use crate::telemetry::spawn_blocking_with_tracing;
use crate::{domain::SubscriberEmail, email_client::EmailClient, routes::error_chain_fmt};
use actix_web::body::BoxBody;
use actix_web::http::header::{HeaderMap, HeaderValue};
use actix_web::http::{header, StatusCode};
use actix_web::{web, HttpRequest, HttpResponse, ResponseError};
use anyhow::Context;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use base64::{engine::general_purpose, Engine as _};
use secrecy::{ExposeSecret, SecretString};
use sqlx::PgPool;
use tokio::task::JoinHandle;
use uuid::Uuid;

// Newsletter request body data. We derive Deserialize to parse the incoming request body. parsing is done automatically by Actix Web. (parsing means converting the raw HTTP request body into a Rust data structure)
#[derive(serde::Deserialize)]
pub struct BodyData {
    title: String,
    content: Content,
}

#[derive(serde::Deserialize)]
pub struct Content {
    text: String,
    html: String,
}

struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

async fn get_confirmed_subscribers(
    pool: &PgPool,
) -> Result<Vec<Result<ConfirmedSubscriber, anyhow::Error>>, anyhow::Error> {
    // The return type is a vector of Results because we want to capture potential errors when parsing email addresses or fetching data from the database or other unexpected errors(like network issues, database connection issues, etc.)

    let confirmed_subscribers = sqlx::query!(
        r#"
        SELECT email
        FROM subscriptions
        WHERE status = 'confirmed'
        "#,
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| match SubscriberEmail::parse(r.email) {
        Ok(email) => Ok(ConfirmedSubscriber { email }),
        Err(error) => Err(anyhow::anyhow!(error)), // Convert the parsing error into anyhow::Error
    })
    .collect();

    Ok(confirmed_subscribers)
}

#[derive(thiserror::Error)]
pub enum PublishError {
    #[error("Authentication failed.")]
    AuthError(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for PublishError {
    // fn status_code(&self) -> reqwest::StatusCode {
    //     match self {
    //         PublishError::UnexpectedError(_) => http::StatusCode::INTERNAL_SERVER_ERROR,
    //         PublishError::AuthError(_) => http::StatusCode::UNAUTHORIZED,
    //     }
    // }

    // status code is invoked by default implementation of error_response
    fn error_response(&self) -> HttpResponse<BoxBody> {
        match self {
            PublishError::UnexpectedError(_) => {
                HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR)
            }
            PublishError::AuthError(_) => {
                let mut response = HttpResponse::new(StatusCode::UNAUTHORIZED);
                let header_value = HeaderValue::from_str(r#"Basic realm="publish""#).unwrap();
                response
                    .headers_mut()
                    .insert(header::WWW_AUTHENTICATE, header_value);
                response
            }
        }
    }
}

#[derive(Debug)]
struct Credentials {
    username: String,
    password: SecretString,
}

/// Extracts basic authentication credentials from the request headers.
/// Returns an error if the `Authorization` header is missing or malformed.
fn basic_authentication(headers: &HeaderMap) -> Result<Credentials, anyhow::Error> {
    println!("headers: {:?}", headers);
    let header_value = headers
        .get("Authorization")
        .context("The 'Authorization' header was missing")?
        .to_str()
        .context("The 'Authorization' header was not a valid UTF-8 string")?;

    let base64encoded_segment = header_value
        .strip_prefix("Basic ") // Remove the "Basic " prefix
        .context("The 'Authorization' header is not a Basic authentication")?; // Extract the base64 encoded part which is usually after "Basic <encoded_string>"
    let decoded_bytes = general_purpose::STANDARD
        .decode(base64encoded_segment)
        .context("Failed to decode the 'Authorization' header")?; // Decode the base64 encoded string which results in vector of bytes
    let decoded_credentials = String::from_utf8(decoded_bytes)
        .context("The decoded 'Authorization' header is not a valid UTF-8 string")?;
    // Convert the decoded bytes into a UTF-8 string which is usually in the format "username:password"

    let mut credentials = decoded_credentials.splitn(2, ':'); // Split the string into username and password using ':' as the delimiter
    let username = credentials
        .next()
        .ok_or_else(|| anyhow::anyhow!("A username must be provided in 'Basic' auth."))?
        .to_string(); // Extract the username part
    let password = credentials
        .next()
        .ok_or_else(|| anyhow::anyhow!("A password must be provided in 'Basic' auth."))?
        .to_string(); // Extract the password part

    Ok(Credentials {
        username,
        password: SecretString::new(Box::from(password)),
    })
}

#[tracing::instrument(name = "Validate credentials", skip(pool, credentials))]
async fn validate_credentials(
    credentials: Credentials,
    pool: &PgPool,
) -> Result<Uuid, PublishError> {
    let (user_id, expected_password_hash) = get_stored_credentials(&credentials.username, pool)
        .await
        .map_err(PublishError::UnexpectedError)?
        .ok_or_else(|| PublishError::AuthError(anyhow::anyhow!("Unknown username.")))?;

    // Verify the password using Argon2
    // Note that we do not need to provide the salt and other parameters because they are already embedded in the PHC string format.
    // Therefore, the parsing we did above extracts all the necessary information for verification.
    // This roughly takes about 288ms which is 0.288 seconds to compute and verify the hash. This could lead to blocking problem if we have many concurrent requests. Possible solutions are to offload the computation to a separate thread pool (which we have done) or use a faster hashing algorithm.
    // The current span is needed if we need to trace our execution flow.
    spawn_blocking_with_tracing(|| {
        verify_password_hash(expected_password_hash, credentials.password)
    })
    .await
    .context("Failed to join password verification task.") // Meaning; we failed to wait for the spawned blocking task to finish
    .map_err(PublishError::UnexpectedError)??; // The double ?? is because the first one is for the JoinError and the second one is for the Result returned by verify_password_hash

    Ok(user_id)
}

#[tracing::instrument(
    name = "Verify password hash",
    skip(expected_password_hash, password_candidate)
)]
fn verify_password_hash(
    expected_password_hash: SecretString,
    password_candidate: SecretString,
) -> Result<(), PublishError> {
    let expected_password_hash = PasswordHash::new(expected_password_hash.expose_secret())
        .context("Failed to parse the stored password hash to the PHC string format.")
        .map_err(PublishError::UnexpectedError)?; // Basically converting the expected password hash string into PasswordHash struct

    Argon2::default()
        .verify_password(
            password_candidate.expose_secret().as_bytes(),
            &expected_password_hash,
        )
        .context("Invalid password.")
        .map_err(PublishError::AuthError) // This now compares the password candidate with the expected password hash
}

async fn get_stored_credentials(
    username: &str,
    pool: &PgPool,
) -> Result<Option<(Uuid, SecretString)>, anyhow::Error> {
    let row = sqlx::query!(
        r#"
	SELECT user_id, password_hash
	FROM users
	WHERE username = $1
	"#,
        username,
    )
    .fetch_optional(pool)
    .await
    .context("Failed to perform a query to retrieve stored credentials.")?
    .map(|row| (row.user_id, SecretString::new(row.password_hash.into())));

    Ok(row)
}

// We will now use an extractor to parse the incoming request body into our BodyData struct
// Help us to know who is calling this function and with what parameters
// name -> Name of the span
// skip -> We don't want to include these parameters in the span
// fields -> We will fill these fields later
#[tracing::instrument(
    name = "Publish a newsletter issue",
    skip(body, pool, email_client, request) ,
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn publish_newsletter(
    body: web::Json<BodyData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    request: HttpRequest,
) -> Result<HttpResponse, PublishError> {
    let credentials =
        basic_authentication(request.headers()).map_err(|e| PublishError::AuthError(e))?;
    tracing::Span::current().record("username", &tracing::field::display(&credentials.username)); // Record the username in the tracing span

    let user_id = validate_credentials(credentials, &pool).await?;
    println!("user_id: {:?}", user_id);
    tracing::Span::current().record("user_id", &tracing::field::display(&user_id)); // Record the user_id in the tracing span

    let subscribers = get_confirmed_subscribers(&pool).await?;

    for subscriber in subscribers {
        match subscriber {
            Ok(subscriber) => {
                let _ = email_client
                    .send_email(
                        &subscriber.email,
                        &body.title,
                        &body.content.html,
                        &body.content.text,
                    )
                    .await
                    .with_context(|| format!("Failed to send newsletter to {}", &subscriber.email));
            }
            Err(error) => {
                tracing::warn!(
                    // Log the entire cause chain for better debugging
                    error.cause_chain = ?error,
                    // Log a warning message with the error details. Use \ to break the line for better readability
                    "Skipping a confirmed subscriber. \
                    Their stored email address is invalid."
                )
            }
        }
        // The difference between context and with_context is that with_context is lazy. It takes a closure as argument and the closure is only called in case of an error.
    }

    Ok(HttpResponse::Ok().finish())
}
