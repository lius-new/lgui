#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GraphicsPreference {
    #[default]
    Auto,
    OpenGl,
    Vulkan,
    Metal,
    Software,
}

impl GraphicsPreference {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::OpenGl => "opengl",
            Self::Vulkan => "vulkan",
            Self::Metal => "metal",
            Self::Software => "software",
        }
    }
}
