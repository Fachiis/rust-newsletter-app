use crate::configuration::{DatabaseSettings, Settings};
use crate::email_client::EmailClient;
use crate::routes::{
    confirm, health_check, home, login, login_form, publish_newsletter, subscribe,
};
use actix_session::{storage::RedisSessionStore, SessionMiddleware};
use actix_web::cookie::Key;
use actix_web::dev::Server;
use actix_web::{web, App, HttpServer};
use actix_web_flash_messages::storage::CookieMessageStore;
use actix_web_flash_messages::FlashMessagesFramework;
use secrecy::ExposeSecret;
use secrecy::SecretString;
use sqlx::PgPool;
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

pub async fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
    let connection_pool = if let Ok(database_url) = std::env::var("DATABASE_URL") {
        tracing::info!("Connecting to DB with the DATABASE_URL from env variable.");
        PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to Postgres")
    } else {
        tracing::info!("Connecting to DB with the configuration file.");
        PgPool::connect_with(configuration.with_db())
            .await
            .expect("Failed to connect to Postgres")
    };

    connection_pool
}

pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    pub async fn build(configuration: Settings) -> Result<Self, anyhow::Error> {
        // Set up the db pool connection
        let connection_pool = get_connection_pool(&configuration.database).await;

        // Set up the email client
        let sender_email = configuration
            .email_client
            .sender()
            .expect("Invalid sender email address.");
        let timeout = configuration.email_client.timeout();
        let email_client = EmailClient::new(
            configuration.email_client.base_url,
            sender_email,
            configuration.email_client.authorization_token,
            timeout,
        );

        // Get the port number
        let address = format!(
            "{}:{}",
            configuration.application.host, configuration.application.port
        );
        let listener = TcpListener::bind(address)?;
        let port = listener.local_addr()?.port();
        let server = run(
            listener,
            connection_pool,
            email_client,
            configuration.application.base_url,
            HmacSecret(configuration.application.hmac_secret),
            configuration.redis_uri,
        )
        .await?;

        // We save the port number and server instance for later use
        Ok(Self { port, server })
    }

    // getter for port number
    pub fn port(&self) -> u16 {
        self.port
    }

    // run the server until it is stopped
    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        self.server.await
    }
}

// Define a wrapper type in order to retrieve the base URL in the handlers
// using actix-web's dependency injection system
// Retrieving a String directly would work, but it will expose us to potential conflicts.
// By defining a new type, we ensure that there are no conflicts with other String dependencies.
pub struct ApplicationBaseUrl(pub String);

async fn run(
    listener: TcpListener,
    db_pool: PgPool,
    email_client: EmailClient,
    base_url: String,
    hmac_secret: HmacSecret,
    redis_uri: SecretString,
) -> Result<Server, anyhow::Error> {
    // web::Data is a smart pointer Arc<T> around a type T that allows sharing
    // state across different handlers in a thread-safe way.
    // With this, we have a cheap clone of the pointer instead of cloning the whole connection,
    // and only one connection is created for the whole application.
    let db_pool = web::Data::new(db_pool);
    let email_client = web::Data::new(email_client);
    let base_url = web::Data::new(ApplicationBaseUrl(base_url));

    let secret_key = Key::from(hmac_secret.0.expose_secret().as_bytes());
    let message_store = CookieMessageStore::builder(secret_key.clone()).build(); // Create a message store for flash messages using cookies. This will allow us to store flash messages in cookies and retrieve them in subsequent requests. The Key is used to encrypt and decrypt the flash messages stored in the cookies, ensuring that the messages are secure and cannot be tampered with by the client.
    let message_framework = FlashMessagesFramework::builder(message_store).build();

    let redis_store = RedisSessionStore::new(redis_uri.expose_secret()).await?;

    // Beware: app instance is created for each worker thread -  the cost of a string allocation (or a pointer clone) is negligible compared to the cost of handling a request - so it's ok to clone the db_pool here
    let server = HttpServer::new(move || {
        App::new()
            .wrap(message_framework.clone()) // Add the flash messages framework middleware to handle flash messages across requests.
            .wrap(SessionMiddleware::new(redis_store.clone(), secret_key.clone()))
            .wrap(TracingLogger::default()) // Add the tracing logger middleware to log incoming requests and their details, such as the request method, path, response status, and processing time. This is useful for monitoring and debugging purposes, as it provides insights into the behavior of the application and helps identify potential issues or bottlenecks.
            .route("/", web::get().to(home))
            .route("/login", web::get().to(login_form))
            .route("/login", web::post().to(login))
            .route("/newsletters", web::post().to(publish_newsletter))
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            .route("/subscriptions/confirm", web::get().to(confirm))
            .app_data(db_pool.clone()) // Register the DB connection as part of the application state: stateful remember of the DB connection
            .app_data(email_client.clone()) // Register the email client as part of the application state
            .app_data(base_url.clone())
            .app_data(web::Data::new(hmac_secret.clone())) // Register the HMAC secret as part of the application state
    })
    .listen(listener)?
    .run();

    Ok(server)
}

#[derive(Clone, Debug)]
pub struct HmacSecret(pub SecretString);
