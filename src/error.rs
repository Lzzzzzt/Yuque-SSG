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
    #[error("Yuque Client Error: {0}")]
    Yuque(String),
    #[error("Internal Error: {0}")]
    Internal(String),
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
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

impl From<yuque_rust::YuqueError> for Error {
    fn from(value: yuque_rust::YuqueError) -> Self {
        Self::Yuque(value.to_string())
    }
}

impl From<crate::toc::FrontmatterBuilderError> for Error {
    fn from(value: crate::toc::FrontmatterBuilderError) -> Self {
        Self::Internal(value.to_string())
    }
}

impl From<crate::toc::SidebarItemBuilderError> for Error {
    fn from(value: crate::toc::SidebarItemBuilderError) -> Self {
        Self::Internal(value.to_string())
    }
}
