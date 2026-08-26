use std::sync::Arc;

use crate::core::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorTokens {
    pub surface: Color,
    pub surface_raised: Color,
    pub surface_interactive: Color,
    pub surface_hover: Color,
    pub border: Color,
    pub border_subtle: Color,
    pub text: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub accent_contrast: Color,
}

impl Default for ColorTokens {
    fn default() -> Self {
        Self {
            surface: Color(0x15181D),
            surface_raised: Color(0x242931),
            surface_interactive: Color(0x2B313A),
            surface_hover: Color(0x353D48),
            border: Color(0x53606F),
            border_subtle: Color(0x3C4653),
            text: Color(0xF3F5F7),
            text_secondary: Color(0xC4CBD4),
            text_muted: Color(0x8F9AA8),
            accent: Color(0x3CB8C5),
            accent_contrast: Color(0x071416),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpacingTokens {
    pub xs: i32,
    pub sm: i32,
    pub md: i32,
    pub lg: i32,
    pub xl: i32,
}

impl Default for SpacingTokens {
    fn default() -> Self {
        Self {
            xs: 4,
            sm: 8,
            md: 12,
            lg: 16,
            xl: 24,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypographyTokens {
    pub body_size: i32,
    pub label_size: i32,
    pub body_weight: u16,
    pub strong_weight: u16,
}

impl Default for TypographyTokens {
    fn default() -> Self {
        Self {
            body_size: -14,
            label_size: -12,
            body_weight: 400,
            strong_weight: 700,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThemeTokens {
    pub colors: ColorTokens,
    pub spacing: SpacingTokens,
    pub typography: TypographyTokens,
}

#[derive(Clone)]
pub struct ThemeContext {
    tokens: Arc<ThemeTokens>,
}

impl ThemeContext {
    pub fn new(tokens: ThemeTokens) -> Self {
        Self {
            tokens: Arc::new(tokens),
        }
    }

    pub fn tokens(&self) -> &ThemeTokens {
        &self.tokens
    }
}

impl Default for ThemeContext {
    fn default() -> Self {
        Self::new(ThemeTokens::default())
    }
}

impl PartialEq for ThemeContext {
    fn eq(&self, other: &Self) -> bool {
        self.tokens == other.tokens
    }
}
