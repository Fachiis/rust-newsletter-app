use sqlx::PgPool;
use std::net::TcpListener;
use zero2prod::configuration::get_configuration;
use zero2prod::email_client::EmailClient;
use zero2prod::startup::run;
use zero2prod::telemetry::{get_subscriber, init_subscriber};

// Entry point of the application
#[tokio::main]
async fn main() -> std::io::Result<()> {
    // span data tracing init
    let subscriber = get_subscriber("zero2prod".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);

    // Panic if we can't read configuration
    let configuration = get_configuration().expect("Failed to read configuration");
    // Create connection pool. Use the DATABASE_URL env variable if it exists for flexibility in deployment scenarios OR fall back to config file.
    let connection_pool = if let Ok(database_url) = std::env::var("DATABASE_URL") {
        tracing::info!("Connecting to DB with the DATABASE_URL from env variable.");
        PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to Postgres")
    } else {
        tracing::info!("Connecting to DB with the configuration file.");
        PgPool::connect_with(configuration.database.with_db())
            .await
            .expect("Failed to connect to Postgres")
    };

    let sender_email = configuration
        .email_client
        .sender()
        .expect("Invalid sender email address.");
    let email_client = EmailClient::new(
        configuration.email_client.base_url,
        sender_email,
        configuration.email_client.authorization_token,
    );

    // Get the port number
    let address = format!(
        "{}:{}",
        configuration.application.host, configuration.application.port
    );

    // Here we propagate the error rather and not panic
    let listener = TcpListener::bind(address)?;
    run(listener, connection_pool, email_client)?.await
}
