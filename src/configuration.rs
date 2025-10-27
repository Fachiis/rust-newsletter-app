use secrecy::{ExposeSecret, SecretBox};

#[derive(serde::Deserialize)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub database: DatabaseSettings,
}

#[derive(serde::Deserialize)]
pub struct ApplicationSettings {
    pub port: u16,
    pub host: String,
}

#[derive(serde::Deserialize)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: SecretBox<String>,
    pub port: u16,
    pub host: String,
    pub database_name: String,
}

impl DatabaseSettings {
    pub fn connection_string(&self) -> SecretBox<String> {
        SecretBox::new(Box::new(format!(
            "postgres://{}:{}@{}:{}/{}",
            self.username,
            self.password.expose_secret(),
            self.host,
            self.port,
            self.database_name
        )))
    }

    pub fn connection_string_without_db_name(&self) -> SecretBox<String> {
        SecretBox::new(Box::new(format!(
            "postgres://{}:{}@{}:{}",
            self.username,
            self.password.expose_secret(),
            self.host,
            self.port
        )))
    }
}

pub fn get_configuration() -> Result<Settings, config::ConfigError> {
    // Initialize our config reader
    // let settings = config::Config::builder()
    //     .add_source(config::File::with_name("configuration"))
    //     .build()?
    //     .try_deserialize()?; // propagate the error with ?
    //
    // Ok(settings)
    let mut settings = config::Config::builder();
    let base_path = std::env::current_dir().expect("Failed to determine the current directory");
    let configuration_directory = base_path.join("configuration");

    // read and merge the base configuration file
    settings = settings
        .add_source(config::File::from(configuration_directory.join("base")).required(true));

    // Detect the running environment. Default to "local" if not set
    let environment: Environment = std::env::var("APP_ENVIRONMENT")
        .unwrap_or_else(|_| "local".into())
        .try_into()
        .expect("Failed to parse APP_ENVIRONMENT.");

    // Base on the detected environment, load the corresponding configuration file
    settings = settings.add_source(
        config::File::from(configuration_directory.join(environment.as_str())).required(true),
    );

    // Build the configuration
    let settings = settings.build()?;

    // Deserialize the configuration
    settings.try_deserialize()
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
