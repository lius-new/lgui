use std::sync::Arc;

#[cfg(feature = "svg")]
use crate::core::PhysicalSize;
use crate::core::{CustomPaintStyle, ScenePrimitive, UiRect};

use super::AssetError;
#[cfg(feature = "svg")]
use super::ImageData;

pub trait CustomPaintProvider: Send + Sync + 'static {
    fn record(
        &self,
        key: &str,
        bounds: UiRect,
        style: CustomPaintStyle,
    ) -> Result<Option<SceneFragment>, AssetError>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneFragment {
    commands: Arc<[ScenePrimitive]>,
}

impl SceneFragment {
    pub fn new(commands: impl Into<Arc<[ScenePrimitive]>>) -> Self {
        Self {
            commands: commands.into(),
        }
    }

    pub fn commands(&self) -> &[ScenePrimitive] {
        &self.commands
    }
}

#[cfg(feature = "svg")]
pub trait SvgRenderer: Send + Sync + 'static {
    fn render(&self, source: &[u8], size: PhysicalSize) -> Result<ImageData, AssetError>;
}
