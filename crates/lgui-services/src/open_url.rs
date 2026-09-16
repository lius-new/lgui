use std::{fmt, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenUrlError(String);

impl OpenUrlError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for OpenUrlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for OpenUrlError {}

pub trait UrlOpener: Send + Sync + 'static {
    fn open(&self, url: &str) -> Result<(), OpenUrlError>;
}

#[derive(Clone)]
pub struct OpenUrlHandle(Arc<dyn UrlOpener>);

impl OpenUrlHandle {
    pub fn new(opener: impl UrlOpener) -> Self {
        Self(Arc::new(opener))
    }

    pub fn open(&self, url: &str) -> Result<(), OpenUrlError> {
        self.0.open(url)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemUrlOpener;

impl UrlOpener for SystemUrlOpener {
    fn open(&self, url: &str) -> Result<(), OpenUrlError> {
        validate_external_url(url)?;
        open::that_detached(url).map_err(|error| OpenUrlError::new(error.to_string()))
    }
}

pub fn system_url_opener() -> OpenUrlHandle {
    OpenUrlHandle::new(SystemUrlOpener)
}

pub fn open_url(url: &str) -> Result<(), OpenUrlError> {
    SystemUrlOpener.open(url)
}

fn validate_external_url(url: &str) -> Result<(), OpenUrlError> {
    let scheme = url.split_once(':').map(|(scheme, _)| scheme);
    if !matches!(scheme, Some("http" | "https" | "mailto")) {
        Err(OpenUrlError::new(
            "external URLs must use http, https, or mailto",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "open_url_test.rs"]
mod tests;
