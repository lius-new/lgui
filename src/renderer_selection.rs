#[cfg(feature = "renderer-skia")]
use lgui_render_api::GraphicsPreference;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererKind {
    #[cfg(all(feature = "renderer-gdi", target_os = "windows"))]
    Gdi,
    #[cfg(all(feature = "renderer-d2d", target_os = "windows"))]
    D2d,
    #[cfg(feature = "renderer-skia")]
    Skia(GraphicsPreference),
}

impl Default for RendererKind {
    fn default() -> Self {
        #[cfg(all(feature = "renderer-gdi", target_os = "windows"))]
        {
            return Self::Gdi;
        }
        #[cfg(all(
            not(all(feature = "renderer-gdi", target_os = "windows")),
            feature = "renderer-d2d",
            target_os = "windows"
        ))]
        {
            return Self::D2d;
        }
        #[cfg(all(
            not(all(feature = "renderer-gdi", target_os = "windows")),
            not(all(feature = "renderer-d2d", target_os = "windows")),
            feature = "renderer-skia"
        ))]
        {
            Self::Skia(GraphicsPreference::Auto)
        }
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
            #[cfg(all(feature = "renderer-gdi", target_os = "windows"))]
            Self::Gdi => "gdi",
            #[cfg(all(feature = "renderer-d2d", target_os = "windows"))]
            Self::D2d => "d2d",
            #[cfg(feature = "renderer-skia")]
            Self::Skia(_) => "skia",
        }
    }

    pub fn probe(self) -> Result<(), RendererProbeError> {
        match self {
            #[cfg(all(feature = "renderer-gdi", target_os = "windows"))]
            Self::Gdi => Ok(()),
            #[cfg(all(feature = "renderer-d2d", target_os = "windows"))]
            Self::D2d => lgui_render_d2d::probe_d2d_support()
                .map_err(|error| RendererProbeError(error.to_string())),
            #[cfg(feature = "renderer-skia")]
            Self::Skia(preference) => {
                lgui_render_skia::probe_skia_support(preference).map_err(RendererProbeError)
            }
        }
    }
}
