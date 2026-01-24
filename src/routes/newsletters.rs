use actix_web::{http, web, HttpResponse, ResponseError};
use anyhow::Context;
use sqlx::PgPool;

use crate::{domain::SubscriberEmail, email_client::EmailClient, routes::error_chain_fmt};

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
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for PublishError {
    fn status_code(&self) -> reqwest::StatusCode {
        match self {
            PublishError::UnexpectedError(_) => http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

// We will now use an extractor to parse the incoming request body into our BodyData struct
pub async fn publish_newsletter(
    body: web::Json<BodyData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
) -> Result<HttpResponse, PublishError> {
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
