use lgui_render_api::GraphicsPreference;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererKind {
    Skia(GraphicsPreference),
}

impl Default for RendererKind {
    fn default() -> Self {
        Self::Skia(GraphicsPreference::Auto)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RendererProbeError(String);

impl std::fmt::Display for RendererProbeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for RendererProbeError {}

impl RendererKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skia(_) => "skia",
        }
    }

    pub fn probe(self) -> Result<(), RendererProbeError> {
        match self {
            Self::Skia(preference) => {
                lgui_render_skia::probe_skia_support(preference).map_err(RendererProbeError)
            }
        }
    }
}
