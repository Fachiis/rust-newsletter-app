use crate::authentication::{validate_credentials, AuthError, Credentials};
use crate::{domain::SubscriberEmail, email_client::EmailClient, routes::error_chain_fmt};
use actix_web::body::BoxBody;
use actix_web::http::header::{HeaderMap, HeaderValue};
use actix_web::http::{header, StatusCode};
use actix_web::{web, HttpRequest, HttpResponse, ResponseError};
use anyhow::Context;
use base64::{engine::general_purpose, Engine as _};
use secrecy::SecretString;
use sqlx::PgPool;

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

/// Get the list of confirmed subscribers from the database.
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

/// Extracts basic authentication credentials from the request headers.
/// Returns an error if the `Authorization` header is missing or malformed.
fn basic_authentication(headers: &HeaderMap) -> Result<Credentials, anyhow::Error> {
    // Get the "Authorization" header from the request headers. If it is missing, return an error with context.
    println!("headers: {:?}", headers);
    let header_value = headers
        .get("Authorization")
        .context("The 'Authorization' header was missing")?
        .to_str()
        .context("The 'Authorization' header was not a valid UTF-8 string")?;

    // The "Authorization" header is expected to be in the format "Basic <base64encoded_credentials>"
    let base64encoded_segment = header_value
        .strip_prefix("Basic ") // Remove the "Basic " prefix
        .context("The 'Authorization' header is not a Basic authentication")?; // ? If the prefix is not present, return an error with context

    // The remaining part is the base64 encoded credentials. We need to decode it to get the username and password.
    let decoded_bytes = general_purpose::STANDARD
        .decode(base64encoded_segment)
        .context("Failed to decode the 'Authorization' header")?;

    // The decoded bytes should be in the format "username:password". We need to convert it into a string and then split it to get the username and password.
    let decoded_credentials = String::from_utf8(decoded_bytes)
        .context("The decoded 'Authorization' header is not a valid UTF-8 string")?;

    // Split the decoded credentials into username and password. We use splitn to split the string into at most 2 parts, so that we can handle cases where the password contains ':' character.
    let mut credentials = decoded_credentials.splitn(2, ':');

    // Extract the username and password from the split credentials. If either of them is missing, return an error with context.
    let username = credentials
        .next()
        .ok_or_else(|| anyhow::anyhow!("A username must be provided in 'Basic' auth."))?
        .to_string();
    let password = credentials
        .next()
        .ok_or_else(|| anyhow::anyhow!("A password must be provided in 'Basic' auth."))?
        .to_string();

    Ok(Credentials {
        username,
        password: SecretString::new(Box::from(password)),
    })
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
    // Extract the credentials from the "Authorization" header. If the header is missing or malformed, return an error with context.
    let credentials =
        basic_authentication(request.headers()).map_err(|e| PublishError::AuthError(e))?;
    tracing::Span::current().record("username", &tracing::field::display(&credentials.username)); // Record the username in the tracing span

    // Validate the credentials against the database. If the credentials are invalid, return an error with context.
    let user_id = validate_credentials(credentials, &pool)
        .await
        .map_err(|e| match e {
            AuthError::InvalidCredentials(_) => PublishError::AuthError(e.into()), // Convert the error into PublishError::AuthError
            AuthError::UnexpectedError(_) => PublishError::UnexpectedError(e.into()),
        })?;
    tracing::Span::current().record("user_id", &tracing::field::display(&user_id)); // Record the user_id in the tracing span

    // Get the list of confirmed subscribers from the database. If there is an error during the query, return an error with context.
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
