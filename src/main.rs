use sqlx::PgPool;
use std::net::TcpListener;
use zero2prod::configuration::get_configuration;
use zero2prod::startup::run;
use zero2prod::telemetry::{get_subscriber, init_subscriber};

// Entry point of the application
#[tokio::main]
async fn main() -> std::io::Result<()> {
    // span data tracing init
    let subscriber = get_subscriber("zero2prod".into(), "info".into());
    init_subscriber(subscriber);

    // Panic if we can't read configuration
    let configuration = get_configuration().expect("Failed to read configuration");

    // connect to the db pool
    let connection_pool = PgPool::connect(&configuration.database.connection_string())
        .await
        .expect("Failed to connect to Postgres.");

    // Get the port number
    let address = format!("127.0.0.1:{}", configuration.application_port);

    // Here we propagate the error rather and not panic
    let listener = TcpListener::bind(address)?;
    run(listener, connection_pool)?.await
}
