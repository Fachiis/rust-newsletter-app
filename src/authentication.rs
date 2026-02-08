use crate::telemetry::spawn_blocking_with_tracing;
use anyhow::Context;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use secrecy::{ExposeSecret, SecretString};
use sqlx::PgPool;
use uuid::Uuid;

/// Credentials used for authentication.
#[derive(Debug)]
pub struct Credentials {
    pub username: String,
    pub password: SecretString,
}

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials(#[source] anyhow::Error),
    #[error(transparent)]
    // This variant is used to wrap any unexpected errors that may occur during authentication
    UnexpectedError(#[from] anyhow::Error),
}

/// Verifies the password hash using Argon2. Returns Ok(()) if the password is correct, otherwise returns an error with context.
#[tracing::instrument(
    name = "Verify password hash",
    skip(expected_password_hash, password_candidate)
)]
fn verify_password_hash(
    expected_password_hash: SecretString,
    password_candidate: SecretString,
) -> Result<(), AuthError> {
    // The expected_password_hash is in the PHC string format which contains all the necessary information for verification (like salt, iterations, memory cost, etc.). Therefore, we can directly use it for verification without needing to provide additional parameters.
    let expected_password_hash = PasswordHash::new(expected_password_hash.expose_secret())
        .context("Failed to parse the stored password hash to the PHC string format.")?; // Basically converting the expected password hash string into PasswordHash struct

    // Use Argon2 to verify the password candidate against the expected password hash. If the verification fails, return an error with context.
    Argon2::default()
        .verify_password(
            password_candidate.expose_secret().as_bytes(),
            &expected_password_hash,
        )
        .context("Invalid password.")
        .map_err(AuthError::InvalidCredentials) // Convert the error into AuthError::InvalidCredentials
}

/// Get the stored credentials for a given username from the database. Returns None if the username does not exist. Returns an error if there is an issue with the database query or if there is an unexpected error.
#[tracing::instrument(name = "Get stored credentials", skip(pool))]
pub async fn get_stored_credentials(
    username: &str,
    pool: &PgPool,
) -> Result<Option<(Uuid, SecretString)>, anyhow::Error> {
    // Query the database for the user with the given username. If the user is not found, return Ok(None). If there is an error during the query, return an error with context.
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

#[tracing::instrument(name = "Validate credentials", skip(pool, credentials))]
pub async fn validate_credentials(
    credentials: Credentials,
    pool: &PgPool,
) -> Result<Uuid, AuthError> {
    let mut user_id = None;
    let mut expected_password_hash = SecretString::new(Box::from(
        "$argon2id$v=19$m=15000,t=2,p=1$\
gZiV/M1gPc22ElAH/Jh1Hw$\
CWOrkoo7oJBQ/iyh7uJ0LO2aLEfrHwTWllSAxT0zRno"
            .to_string(),
    ));

    // Get the stored credentials for the g`iven username from the database.
    if let Some((stored_user_id, stored_password_hash)) =
        get_stored_credentials(&credentials.username, pool).await?
    {
        user_id = Some(stored_user_id);
        expected_password_hash = stored_password_hash;
    }

    // Verify the password using Argon2
    // Note that we do not need to provide the salt and other parameters because they are already embedded in the PHC string format.
    // Therefore, the parsing we did above extracts all the necessary information for verification.
    // This roughly takes about 288ms which is 0.288 seconds to compute and verify the hash. This could lead to blocking problem if we have many concurrent requests. Possible solutions are to offload the computation to a separate thread pool (which we have done) or use a faster hashing algorithm.
    // The current span is needed if we need to trace our execution flow.
    spawn_blocking_with_tracing(|| {
        verify_password_hash(expected_password_hash, credentials.password)
    })
    .await
    .context("Failed to spawn blocking task")??; // Meaning; we failed to wait for the spawned blocking task to finish
                                                 // The double ?? is because the first one is for the JoinError and the second one is for the Result returned by verify_password_hash

    user_id.ok_or_else(|| AuthError::InvalidCredentials(anyhow::anyhow!("Unknown username.")))
}
