use std::fmt;

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

pub fn open_url(url: &str) -> Result<(), OpenUrlError> {
    #[cfg(all(feature = "backend-win32", target_os = "windows"))]
    {
        return crate::platform::win32::open_external_url(url)
            .map_err(|error| OpenUrlError::new(error.to_string()));
    }

    #[cfg(not(all(feature = "backend-win32", target_os = "windows")))]
    {
        let _ = url;
        Err(OpenUrlError::new(
            "opening external URLs is unavailable on this backend",
        ))
    }
}
