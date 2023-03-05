use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Can not find `config.yml` in current directory")]
    ConfigNotFound,
    #[error("Can not parse `config.yml` due to {0}")]
    CantParse(String),
    #[error("Missing required value: {0}")]
    MissingFields(String),
    #[error("Io Error: {0}")]
    Io(String),
    #[error("missing required environment: {0}")]
    MissingEnv(String),
    #[error("missing required dependency: {0}")]
    MissingDependency(String),
}

impl From<tokio::io::Error> for Error {
    fn from(value: tokio::io::Error) -> Self {
        match value.kind() {
            std::io::ErrorKind::NotFound => Error::ConfigNotFound,
            _ => Error::Io(value.to_string()),
        }
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(value: serde_yaml::Error) -> Self {
        Self::CantParse(value.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::CantParse(value.to_string())
    }
}
