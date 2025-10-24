use std::borrow::Cow;

use arwen::macho::MachoError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CatError {
    #[error("Cloud error: {0}")]
    Cloud(#[from] CloudError),
    #[error("Hash verify error: {0}")]
    Hash(String),
    #[error("Modify macho error: {0}")]
    Macho(#[from] MachoError),
    #[error("Package error: {0}")]
    Pac(String),
}

#[derive(Error, Debug)]
pub enum CloudError {
    #[error("Deserialize error: {0}")]
    Deserialize(#[from] serde_json::Error),
    #[error("{0}")]
    Api(Cow<'static, str>),
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Request(#[from] RequestError),
}

impl CloudError {
    pub fn api<E>(error: E) -> Self
    where
        Cow<'static, str>: From<E>,
    {
        Self::Api(Cow::from(error))
    }
}

#[derive(Error, Debug)]
pub enum RequestError {
    #[error("HTTP client error: {0}")]
    Client(#[from] reqwest::Error),
    #[error("Request middleware error: {0}")]
    Middleware(#[from] reqwest_middleware::Error),
    #[error("Status error: {0}")]
    Status(String),
}

impl From<reqwest::Error> for CloudError {
    fn from(value: reqwest::Error) -> Self {
        Self::Request(RequestError::Client(value))
    }
}

impl From<reqwest::Error> for CatError {
    fn from(value: reqwest::Error) -> Self {
        CatError::Cloud(CloudError::Request(RequestError::Client(value)))
    }
}

impl From<reqwest_middleware::Error> for CloudError {
    fn from(value: reqwest_middleware::Error) -> Self {
        Self::Request(RequestError::Middleware(value))
    }
}

impl From<reqwest_middleware::Error> for CatError {
    fn from(value: reqwest_middleware::Error) -> Self {
        CatError::Cloud(CloudError::Request(RequestError::Middleware(value)))
    }
}

impl From<std::io::Error> for CatError {
    fn from(value: std::io::Error) -> Self {
        Self::Cloud(CloudError::IO(value))
    }
}
