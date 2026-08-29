#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CompositingLayerBackground {
    Opaque,
    #[default]
    Transparent,
}

/// An independently retained paint surface. Animation is only one possible producer of changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CompositingLayerSpec {
    pub opacity: u8,
    pub background: CompositingLayerBackground,
}

impl CompositingLayerSpec {
    pub const fn new() -> Self {
        Self {
            opacity: 255,
            background: CompositingLayerBackground::Transparent,
        }
    }

    pub const fn opaque(mut self) -> Self {
        self.background = CompositingLayerBackground::Opaque;
        self
    }

    pub const fn transparent(mut self) -> Self {
        self.background = CompositingLayerBackground::Transparent;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        self
    }

    pub fn opacity_f32(self) -> f32 {
        self.opacity as f32 / 255.0
    }
}

impl Default for CompositingLayerSpec {
    fn default() -> Self {
        Self::new()
    }
}
