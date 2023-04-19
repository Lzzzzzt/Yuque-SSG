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
    #[error("Can not install the dependencies")]
    CantInstallDependency,
    #[error("Yuque Client Error: {0}")]
    Yuque(String),
    #[error("Internal Error: {0}")]
    Internal(String),
    #[error("Invalid Schema: {0}")]
    InvalidSchema(String),
    #[error("Invalid Build Command: {0}.")]
    InvalidBuildCommand(String),
    #[error("Run command `{0}` failed.")]
    Build(String),
    #[error("Reqwest Error: {0}")]
    Reqwest(String),
    #[error("Invalid Url: {0}")]
    InvalidUrl(String),
    #[error("Image Error: {0}")]
    Image(String),
    #[error("Can not fetch the theme repo")]
    CantFetchTheme,
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

impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        Self::Reqwest(value.to_string())
    }
}

impl From<url::ParseError> for Error {
    fn from(value: url::ParseError) -> Self {
        Self::InvalidUrl(value.to_string())
    }
}

impl From<image::error::ImageError> for Error {
    fn from(value: image::error::ImageError) -> Self {
        Self::Image(value.to_string())
    }
}
