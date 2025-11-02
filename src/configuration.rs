use secrecy::{ExposeSecret, SecretBox};
use serde_aux::field_attributes::deserialize_number_from_string;
use sqlx::postgres::{PgConnectOptions, PgSslMode};
use sqlx::ConnectOptions;

#[derive(serde::Deserialize)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub database: DatabaseSettings,
}

#[derive(serde::Deserialize)]
pub struct ApplicationSettings {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub host: String,
}

#[derive(serde::Deserialize)]
pub struct DatabaseSettings {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub username: String,
    pub password: SecretBox<String>,
    pub host: String,
    pub database_name: String,
    pub require_ssl: bool,
}

impl DatabaseSettings {
    pub fn without_db(&self) -> PgConnectOptions {
        let ssl_mode = if self.require_ssl {
            PgSslMode::Require
        } else {
            PgSslMode::Prefer
        };

        PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .username(&self.username)
            .password(self.password.expose_secret())
            .ssl_mode(ssl_mode)
    }

    pub fn with_db(&self) -> PgConnectOptions {
        let mut options = self.without_db().database(&self.database_name);
        // for debugging purposes, log all statements
        options = options.log_statements(tracing::log::LevelFilter::Trace);
        options
    }
}

pub fn get_configuration() -> Result<Settings, config::ConfigError> {
    // Determine the base path and configuration directory
    let base_path = std::env::current_dir().expect("Failed to determine the current directory");
    let configuration_directory = base_path.join("configuration");

    // Detect the running environment: defaults to "local"
    let environment: Environment = std::env::var("APP_ENVIRONMENT")
        .unwrap_or_else(|_| "local".into())
        .try_into()
        .expect("Failed to parse APP_ENVIRONMENT.");

    // Start building configuration
    let mut settings = config::Config::builder()
        // Always load the base.yaml (common defaults)
        .add_source(config::File::from(configuration_directory.join("base")).required(true))
        // Load the environment-specific file (local.yaml or production.yaml)
        .add_source(
            config::File::from(configuration_directory.join(environment.as_str())).required(false),
        )
        // Add environment variables (override YAML values)
        .add_source(
            config::Environment::with_prefix("app")
                .separator("__")
                .try_parsing(true)
                .list_separator(",")
                .with_list_parse_key("application.host"),
        );

    // Build and deserialize the merged configuration
    let settings = settings.build()?;
    settings.try_deserialize::<Settings>()
}

/// Possible runtime environments for our application
pub enum Environment {
    Local,
    Production,
}

// Implement a method to get the &str representation of the Environment enum
impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Production => "production",
        }
    }
}

// Implement conversion from String to Environment enum
impl TryFrom<String> for Environment {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Environment::Local),
            "production" => Ok(Environment::Production),
            other => Err(format!(
                "{} is not a supported environment. Use either 'local' or 'production'.",
                other
            )),
        }
    }
}
