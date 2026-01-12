use crate::startup::ApplicationBaseUrl;
use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient,
};
use actix_web::http::StatusCode;
use actix_web::{web, HttpResponse, ResponseError};
use chrono::Utc;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use sqlx::PgPool;
use std::error::Error;
use uuid::Uuid;

// Defining the structure of the expected form data,
// All fields must match the names in the HTML form and they are required by default
#[derive(serde::Deserialize)]
pub struct FormData {
    name: String,
    email: String,
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    fn try_from(value: FormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(value.name)?;
        let email = SubscriberEmail::parse(value.email)?;
        Ok(Self { name, email })
    }
}

/// Generate a random 25-character-long alphanumeric case-sensitive subscription token
fn generate_subscription_token() -> String {
    let mut rng = thread_rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}
// Explanation:
// repeat_with: creates an iterator that repeatedly calls the provided closure to generate values
// map: transforms each sampled value into a char
// take: takes the first 25 characters from the iterator
// collect: collects the characters into a String

// Derive Debug for easier error handling.
// But we cannot derive Display because sqlx::Error does not implement Display
pub struct StoreTokenError(sqlx::Error);

impl Error for StoreTokenError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

impl std::fmt::Debug for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl std::fmt::Display for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "A database error occurred while trying to store a subscription token.",
        )
    }
}

/// Using "source" we can write a function that provides a similar representation for any type that implements Error
fn error_chain_fmt(e: &impl Error, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}\n", e).expect("Failed to write error");
    let mut current = e.source();
    println!("Current error: {:?}", current);
    while let Some(cause) = current {
        write!(f, "Caused by:\n\t {}", cause).expect("Failed to write error");
        current = cause.source();
    }
    Ok(())
}

// Key important things to note:
// The procedural macro #[derive(thiserror::Error)] generates a custom error type
// The #[error] defines the Display representation of the error. You can use placeholders like {0} to refer to fields.
// #[source] used to denote what should be returned as root cause in Error::source.
// #[from] used to automatically implement `From` for the error type.
#[derive(thiserror::Error)]
pub enum SubscribeError {
    #[error("{0}")]
    ValidationError(String),
    #[error("Failed to acquire a postgres connection from the pool.")]
    PoolError(#[source] sqlx::Error),
    #[error("Failed to insert a new subscriber in the database.")]
    InsertSubscriberError(#[source] sqlx::Error),
    #[error("Failed to commit SQL transaction to store a new subscriber.")]
    TransactionError(#[source] sqlx::Error),
    #[error("Failed to store the confirmation token for a new subscriber.")]
    StoreTokenError(#[from] StoreTokenError),
    #[error("Failed to send a confirmation email.")]
    SendEmailError(#[from] reqwest::Error),
}

// Still using the custom bespoke implementation for Debug
// to get nice report using the error source chain
impl std::fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> StatusCode {
        match self {
            SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST,
            SubscribeError::PoolError(_)
            | SubscribeError::InsertSubscriberError(_)
            | SubscribeError::TransactionError(_)
            | SubscribeError::StoreTokenError(_)
            | SubscribeError::SendEmailError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

// tracing instrumentation macro to automatically create and enter spans for us
// name: name of the span
// skip: variables to skip recording in the span
// fields: custom fields to add to the span
#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, pool, email_client, base_url),
    fields(
        subscriber_email=%form.email,
        subscriber_name=%form.name
    )
)]
pub async fn subscribe(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
) -> Result<HttpResponse, SubscribeError> {
    // Why we are using form.0 instead of form.name?
    // Because form is a smart pointer (web::Form) that wraps the actual data (FormData)
    let new_subscriber = form.0.try_into().map_err(SubscribeError::ValidationError)?;

    // Begin a database transaction
    let mut transaction = pool.begin().await.map_err(SubscribeError::PoolError)?;

    let subscriber_id = insert_subscriber(&mut transaction, &new_subscriber)
        .await
        .map_err(SubscribeError::InsertSubscriberError)?;
    let subscription_token = generate_subscription_token();
    store_token(&mut transaction, subscriber_id, &subscription_token)
        .await
        .map_err(SubscribeError::StoreTokenError)?;

    // Commit the transaction else it will be rolled back automatically when dropped
    transaction
        .commit()
        .await
        .map_err(SubscribeError::TransactionError)?;

    // Send the confirmation email
    send_confirmation_email(
        &email_client,
        new_subscriber,
        &base_url.0,
        &subscription_token,
    )
    .await
    .map_err(SubscribeError::SendEmailError)?;

    Ok(HttpResponse::Ok().finish())
}

// Separation of concerns: database interaction logic is separated from request handling logic

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(new_subscriber, transaction)
)]
pub async fn insert_subscriber(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    new_subscriber: &NewSubscriber,
) -> Result<Uuid, sqlx::Error> {
    let subscription_id = Uuid::new_v4();
    sqlx::query!(
        r#"INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, 'pending_confirmation')
        "#,
        subscription_id,
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(),
        Utc::now()
    )
    .execute(transaction.as_mut())
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?; // Using "?" to return early in case of error
    Ok(subscription_id)
}

#[tracing::instrument(
    name = "Store subscription token in the database",
    skip(transaction, subscription_token)
)]
pub async fn store_token(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    subscriber_id: Uuid,
    subscription_token: &str,
) -> Result<(), StoreTokenError> {
    sqlx::query!(
        r#"INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)
        "#,
        subscription_token,
        subscriber_id
    )
    .execute(transaction.as_mut())
    .await
    .map_err(|e| {
        // If an error occurs, we grab the error and emit a log event
        tracing::error!("Failed to execute query: {:?}", e);
        StoreTokenError(e)
    })?;
    Ok(())
}

#[tracing::instrument(
    name = "Send a confirmation email to a new subscriber",
    skip(email_client, new_subscriber)
)]
pub async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), reqwest::Error> {
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, subscription_token
    );

    let plain_body = format!(
        "Welcome to our newsletter!\n\
        Visit {} to confirm your subscription.",
        confirmation_link
    );
    let html_body = format!(
        "Welcome to our newsletter!<br />\
        Click <a href=\"{}\">here</a> to confirm your subscription.",
        confirmation_link
    );

    email_client
        .send_email(new_subscriber.email, "Welcome!", &html_body, &plain_body)
        .await
}
