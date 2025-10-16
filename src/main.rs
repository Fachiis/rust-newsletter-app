use std::net::TcpListener;
use zero2prod::configuration::get_configuration;
use zero2prod::startup::run;

// Entry point of the application
#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Panic if we can't read configuration
    let configuration = get_configuration().expect("Failed to read configuration");

    // Get the port number
    let address = format!("127.0.0.1:{}", configuration.application_port);

    // Here we propagate the error rather and not panic
    let listener = TcpListener::bind(address)?;
    run(listener)?.await
}
