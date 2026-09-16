use std::{fmt, sync::Arc};

use crate::core::{PhysicalSize, UiImageSource};

pub type ImageSource = UiImageSource;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageStatus {
    Loading,
    Ready,
    Failed,
}

pub type AssetBytes = Arc<[u8]>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetError {
    NotFound(String),
    InvalidData(String),
    Unsupported(String),
}

impl fmt::Display for AssetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(formatter, "asset `{id}` was not found"),
            Self::InvalidData(message) | Self::Unsupported(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for AssetError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageData {
    pub size: PhysicalSize,
    pub bgra_premultiplied: AssetBytes,
}

impl ImageData {
    pub fn new(size: PhysicalSize, bytes: impl Into<AssetBytes>) -> Result<Self, AssetError> {
        let bytes = bytes.into();
        let expected = size.width.max(0) as usize * size.height.max(0) as usize * 4;
        if size.width <= 0 || size.height <= 0 || bytes.len() != expected {
            return Err(AssetError::InvalidData(format!(
                "expected {expected} BGRA bytes for {}x{}, received {}",
                size.width,
                size.height,
                bytes.len()
            )));
        }
        Ok(Self {
            size,
            bgra_premultiplied: bytes,
        })
    }
}
