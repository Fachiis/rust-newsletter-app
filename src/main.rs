use sqlx::postgres::PgPoolOptions;
use std::net::TcpListener;
use zero2prod::configuration::get_configuration;
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

    // connect to the db pool
    // connect: creates a connection pool and tries to connect to the DB immediately (async)
    // connect_lazy: creates a connection pool but does not try to connect to the DB until the first query is executed (sync).
    // Set a timeout for acquiring a connection from the pool.Default is 30 seconds.
    let connection_pool = PgPoolOptions::new().connect_lazy_with(configuration.database.with_db());

    // Get the port number
    let address = format!(
        "{}:{}",
        configuration.application.host, configuration.application.port
    );

    // Here we propagate the error rather and not panic
    let listener = TcpListener::bind(address)?;
    run(listener, connection_pool)?.await
}
