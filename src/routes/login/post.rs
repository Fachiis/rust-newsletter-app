use std::fmt::Debug;

use actix_web::cookie::Cookie;
use actix_web::error::InternalError;
use actix_web::http::header::LOCATION;
use actix_web::{web, HttpResponse};
use secrecy::SecretString;
use sqlx::PgPool;

use crate::authentication::{validate_credentials, AuthError, Credentials};
use crate::routes::error_chain_fmt;

#[derive(serde::Deserialize)]
pub struct FormData {
    username: String,
    password: SecretString,
}

#[tracing::instrument(
    name = "Handle login",
    skip(form, pool),
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn login(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, InternalError<LoginError>> {
    // Converting the form data into our Credentials struct. We can directly use the form data to create the Credentials struct since they have the same fields. This is more concise and avoids unnecessary cloning of the data. If we were to clone the data, it would involve creating new instances of the username and password, which is unnecessary since we can directly use the data from the form. By using form.0, we can access the inner FormData struct directly and create the Credentials struct without any additional overhead.

    // let credentials = Credentials {
    //     username: form.username.clone(),
    //     password: form.password.clone(),
    // };

    let credentials = Credentials {
        username: form.0.username,
        password: form.0.password,
    };

    tracing::Span::current().record("username", tracing::field::display(&credentials.username)); // Record the username in the tracing span

    // Validate and authenticate the user credentials
    match validate_credentials(credentials, &pool).await {
        Ok(user_id) => {
            tracing::Span::current().record("user_id", tracing::field::display(&user_id));
            Ok(HttpResponse::SeeOther()
                .insert_header((LOCATION, "/"))
                .finish())
        }
        Err(e) => {
            let e = match e {
                AuthError::InvalidCredentials(_) => LoginError::AuthError(e.into()),
                AuthError::UnexpectedError(_) => LoginError::UnexpectedError(e.into()),
            }; // Convert the AuthError into LoginError

            let response = HttpResponse::SeeOther()
                .insert_header((LOCATION, "/login"))
                .cookie(Cookie::new("_flash", e.to_string()))
                .finish();
            Err(InternalError::from_response(e, response))
        }
    }
}

#[derive(thiserror::Error)]
pub enum LoginError {
    #[error("Authentication failed")]
    AuthError(#[source] anyhow::Error),
    #[error("Something went wrong")]
    UnexpectedError(#[from] anyhow::Error),
}
// Difference between the from and source attributes in the error enum variants:
// The #[from] attribute is used to automatically convert an error of one type into another type when using the ? operator. It allows for easy error propagation without needing to manually convert the error each time. When you use #[from], you can simply return the original error, and it will be automatically converted to the target error type.
// The #[source] attribute is used to indicate the underlying cause of an error. It allows error reporting libraries to traverse the chain of errors and provide more detailed information about the root cause of an error.

// Special implementation of Debug for LoginError to get a nice report using the error source chain. This is useful for debugging and logging purposes, as it allows us to see the entire chain of errors that led to the failure, rather than just the top-level error message. By implementing Debug in this way, we can get a more comprehensive view of the error and its context, which can be invaluable when diagnosing issues in our application.
impl Debug for LoginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}
