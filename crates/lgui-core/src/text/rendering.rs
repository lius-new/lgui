//! Application-supplied text raster parameters, independent of themes and backends.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextHinting {
    None,
    #[default]
    Slight,
    Normal,
    Full,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextRenderingOptions {
    lcd_gamma: Option<u32>,
    lcd_contrast: Option<u32>,
    grayscale_gamma: Option<u32>,
    grayscale_contrast: Option<u32>,
    grayscale_hinting: TextHinting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRenderingError {
    InvalidGamma,
    InvalidContrast,
}

impl std::fmt::Display for TextRenderingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidGamma => "Text gamma must be finite and in [0, 4)",
            Self::InvalidContrast => "Text contrast must be finite and in [0, 1]",
        })
    }
}

impl std::error::Error for TextRenderingError {}

impl TextRenderingOptions {
    pub fn with_grayscale_hinting(mut self, hinting: TextHinting) -> Self {
        self.grayscale_hinting = hinting;
        self
    }

    pub fn grayscale_hinting(self) -> TextHinting {
        self.grayscale_hinting
    }

    pub fn with_grayscale_gamma(mut self, gamma: f32) -> Result<Self, TextRenderingError> {
        // Share the validation and canonical representation with LCD coverage.
        self.grayscale_gamma = Self::default().with_lcd_gamma(gamma)?.lcd_gamma;
        Ok(self)
    }

    pub fn with_grayscale_contrast(mut self, contrast: f32) -> Result<Self, TextRenderingError> {
        self.grayscale_contrast = Self::default().with_lcd_contrast(contrast)?.lcd_contrast;
        Ok(self)
    }

    pub fn grayscale_gamma(self) -> Option<f32> {
        self.grayscale_gamma.map(f32::from_bits)
    }

    pub fn grayscale_contrast(self) -> Option<f32> {
        self.grayscale_contrast.map(f32::from_bits)
    }

    /// Overrides LCD mask-coverage gamma. Zero selects the backend's sRGB curve.
    /// Grayscale and transparent fallback targets retain their native defaults.
    pub fn with_lcd_gamma(mut self, gamma: f32) -> Result<Self, TextRenderingError> {
        if !gamma.is_finite() || !(0.0..4.0).contains(&gamma) {
            return Err(TextRenderingError::InvalidGamma);
        }
        self.lcd_gamma = Some(if gamma == 0.0 { 0 } else { gamma.to_bits() });
        Ok(self)
    }

    pub fn with_lcd_contrast(mut self, contrast: f32) -> Result<Self, TextRenderingError> {
        if !contrast.is_finite() || !(0.0..=1.0).contains(&contrast) {
            return Err(TextRenderingError::InvalidContrast);
        }
        self.lcd_contrast = Some(if contrast == 0.0 {
            0
        } else {
            contrast.to_bits()
        });
        Ok(self)
    }

    pub fn lcd_gamma(self) -> Option<f32> {
        self.lcd_gamma.map(f32::from_bits)
    }

    pub fn lcd_contrast(self) -> Option<f32> {
        self.lcd_contrast.map(f32::from_bits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_coverage_parameters_never_reach_the_native_renderer() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 4.0] {
            assert_eq!(
                TextRenderingOptions::default().with_lcd_gamma(value),
                Err(TextRenderingError::InvalidGamma)
            );
        }
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 1.1] {
            assert_eq!(
                TextRenderingOptions::default().with_lcd_contrast(value),
                Err(TextRenderingError::InvalidContrast)
            );
        }
        let options = TextRenderingOptions::default()
            .with_lcd_gamma(0.0)
            .unwrap()
            .with_lcd_contrast(1.0)
            .unwrap();
        assert_eq!(options.lcd_gamma(), Some(0.0));
        assert_eq!(options.lcd_contrast(), Some(1.0));
        assert_eq!(TextRenderingOptions::default().lcd_gamma(), None);
        assert_eq!(TextRenderingOptions::default().lcd_contrast(), None);
    }

    #[test]
    fn grayscale_parameters_are_validated_and_independent_of_lcd() {
        for gamma in [f32::NAN, f32::INFINITY, -1.0, 4.0] {
            assert!(
                TextRenderingOptions::default()
                    .with_grayscale_gamma(gamma)
                    .is_err()
            );
        }
        for contrast in [f32::NAN, -0.1, 1.1] {
            assert!(
                TextRenderingOptions::default()
                    .with_grayscale_contrast(contrast)
                    .is_err()
            );
        }
        let options = TextRenderingOptions::default()
            .with_lcd_gamma(2.4)
            .unwrap()
            .with_grayscale_gamma(1.6)
            .unwrap()
            .with_grayscale_contrast(0.25)
            .unwrap()
            .with_grayscale_hinting(TextHinting::None);
        assert_eq!(options.lcd_gamma(), Some(2.4));
        assert_eq!(options.grayscale_gamma(), Some(1.6));
        assert_eq!(options.grayscale_contrast(), Some(0.25));
        assert_eq!(options.grayscale_hinting(), TextHinting::None);
        assert_eq!(
            TextRenderingOptions::default().grayscale_hinting(),
            TextHinting::Slight
        );
    }
}
