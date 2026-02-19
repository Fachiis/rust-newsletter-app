use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHasher};
use once_cell::sync::Lazy;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;
use zero2prod::configuration::{get_configuration, DatabaseSettings};
use zero2prod::startup::{get_connection_pool, Application};
use zero2prod::telemetry::{get_subscriber, init_subscriber};

static TRACING: Lazy<()> = Lazy::new(|| {
    let default_filter_level = "info".to_string();
    let subscriber_name = "test".to_string();

    // Initialized once and used for the rest of the tests
    // With test_log env, all logs will be outputted
    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::stdout);
        init_subscriber(subscriber);
    } else {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::sink);
        init_subscriber(subscriber);
    }
});

pub struct TestUser {
    pub user_id: Uuid,
    pub username: String,
    pub password: String,
}

impl TestUser {
    pub fn generate() -> Self {
        Self {
            user_id: Uuid::new_v4(),
            username: Uuid::new_v4().to_string(),
            password: Uuid::new_v4().to_string(),
        }
    }

    async fn store(&self, pool: &PgPool) {
        // Get a salt and hash the password using Argon2
        let salt = SaltString::generate(&mut rand::thread_rng());
        let password_hash = Argon2::default()
            .hash_password(self.password.as_bytes(), &salt)
            .unwrap()
            .to_string();

        sqlx::query!(
            r#"
            INSERT INTO users (user_id, username, password_hash)
            VALUES ($1, $2, $3)
            "#,
            self.user_id,
            self.username,
            password_hash
        )
        .execute(pool)
        .await
        .expect("Failed to store test user.");
    }
}

pub struct TestApp {
    pub address: String,
    pub port: u16,
    pub db_pool: PgPool,
    pub email_server: wiremock::MockServer,
    pub test_user: TestUser,
    pub api_client: reqwest::Client,
}

/// Links embedded in the confirmation email
pub struct ConfirmationLinks {
    pub html: reqwest::Url,
    pub plain_text: reqwest::Url,
}

impl TestApp {
    /// Send a subscription request to the application
    pub async fn post_subscriptions(&self, body: String) -> reqwest::Response {
        self.api_client
            .post(format!("{}/subscriptions", &self.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    /// Send a newsletter request to the application
    pub async fn post_newsletters(&self, body: serde_json::Value) -> reqwest::Response {
        self.api_client
            .post(format!("{}/newsletters", &self.address))
            .basic_auth(&self.test_user.username, Some(&self.test_user.password))
            .json(&body)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn get_confirmation_links(
        &self,
        email_request: &wiremock::Request,
    ) -> ConfirmationLinks {
        let body: serde_json::Value = serde_json::from_slice(&email_request.body).unwrap();

        let get_link = |s: &str| {
            let links: Vec<_> = linkify::LinkFinder::new()
                .links(s)
                .filter(|l| *l.kind() == linkify::LinkKind::Url)
                .collect();
            assert_eq!(links.len(), 1);
            let raw_link = links[0].as_str().to_owned();
            let mut confirmation_link = reqwest::Url::parse(&raw_link).unwrap();
            assert_eq!(confirmation_link.host_str().unwrap(), "127.0.0.1");
            confirmation_link
                .set_port(Some(self.port))
                .expect("Failed to set port");
            confirmation_link
        };

        ConfirmationLinks {
            html: get_link(body["HtmlBody"].as_str().unwrap()),
            plain_text: get_link(body["TextBody"].as_str().unwrap()),
        }
    }

    pub async fn post_login<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .post(format!("{}/login", &self.address))
            .form(body)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn get_login_html(&self) -> String {
        self.api_client
            .get(format!("{}/login", &self.address))
            .send()
            .await
            .expect("Failed to execute request.")
            .text() // Get the response body as text to verify that the error message is properly rendered in the HTML response when we access the login page after a failed login attempt.
            .await
            .expect("Failed to read response body.")
    }
}

/// Configure the database for testing.
async fn configure_database(config: &DatabaseSettings) -> PgPool {
    // Create test DB
    println!("Connecting to Postgres (without DB name)...");
    let mut connection = PgConnection::connect_with(&config.without_db())
        .await
        .expect("Failed to connect to Postgres");
    println!("Connected to Postgres (without DB name).");

    println!("Creating database {}...", config.database_name);
    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await
        .expect("Failed to create database");
    println!("Database {} created.", config.database_name);

    // Migrate test DB
    println!("Connecting to the newly created database...");
    let connection_pool = PgPool::connect_with(config.with_db())
        .await
        .expect("Failed to connect to running postgres instance");
    println!("Connected to the newly created database.");

    println!("Running migrations...");
    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate database");
    println!("Migrations completed.");

    println!("Database setup finished.");

    connection_pool
}

/// Configure or create the app used for each test process which will create a fresh DB connection pool for each test
pub async fn spawn_app() -> TestApp {
    // The first time `initialize` is invoked the code in `TRACING` is executed.
    // All other invocations will instead skip execution.
    Lazy::force(&TRACING);

    let email_server = wiremock::MockServer::start().await;

    // Randomize the database name to avoid conflicts.
    let configuration = {
        let mut c = get_configuration().expect("Failed to read configuration.");
        // Use a different DB for each
        c.database.database_name = Uuid::new_v4().to_string();
        // Use a random OS port
        c.application.port = 0;
        // Point email client to the mock server
        c.email_client.base_url = email_server.uri();
        c
    };

    // Configure test db and run migrations
    configure_database(&configuration.database).await;

    let application = Application::build(configuration.clone())
        .await
        .expect("Failed to build application");
    let application_port = application.port();
    let _ = tokio::spawn(application.run_until_stopped());

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none()) // allow redirects to capture the 303 response
        .cookie_store(true) // Enable cookie store to automatically handle cookies set by the server, such as flash messages, and include them in subsequent requests. This is important for tests that need to verify the presence of cookies or rely on cookies for session management.
        .build()
        .unwrap();

    let test_app = TestApp {
        address: format!("http://127.0.0.1:{}", application_port),
        port: application_port,
        db_pool: get_connection_pool(&configuration.database).await,
        email_server,
        test_user: TestUser::generate(),
        api_client: client,
    };
    // Store the test user in the test database
    test_app.test_user.store(&test_app.db_pool).await;
    test_app
}

/// Utility function to assert that the response is a 303 redirect to the given location
pub fn assert_is_redirect_to(response: &reqwest::Response, location: &str) {
    assert_eq!(
        response.status().as_u16(),
        303,
        "Expected a 303 See Other response, got {}",
        response.status()
    );
    let headers = response.headers();
    assert_eq!(
        headers.get("Location").map(|value| value.to_str().unwrap()),
        Some(location),
        "Expected a Location header in the response, but it was missing."
    );
}
