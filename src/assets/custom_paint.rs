use std::sync::Arc;

use crate::core::{CustomPaintStyle, PhysicalSize, ScenePrimitive, UiRect};

use super::{AssetError, ImageData};

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
