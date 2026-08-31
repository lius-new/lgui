use std::sync::Arc;

use crate::renderer::RenderErrorStage;

use super::WindowId;

type RenderErrorHandler = Arc<dyn Fn(&RenderError) + Send + Sync + 'static>;

#[derive(Clone)]
pub(crate) struct RenderErrorRegistration {
    handler: RenderErrorHandler,
}

impl RenderErrorRegistration {
    pub(crate) fn new(handler: impl Fn(&RenderError) + Send + Sync + 'static) -> Self {
        Self {
            handler: Arc::new(handler),
        }
    }

    pub(crate) fn report(&self, error: &RenderError) {
        (self.handler)(error);
    }
}

/// A renderer failure reported at the application boundary.
///
/// Rendering backends remain independent from the application's logging stack. Applications can
/// install a handler with [`super::Application::on_render_error`] and decide whether a failure
/// belongs in a local log, diagnostics UI, telemetry, or another reporting destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderError {
    window: WindowId,
    renderer: &'static str,
    stage: RenderErrorStage,
    operation: &'static str,
    code: i32,
    message: String,
}

impl RenderError {
    pub(crate) fn new(
        window: WindowId,
        renderer: &'static str,
        stage: RenderErrorStage,
        operation: &'static str,
        code: i32,
        message: impl Into<String>,
    ) -> Self {
        Self {
            window,
            renderer,
            stage,
            operation,
            code,
            message: message.into(),
        }
    }

    pub fn window(&self) -> &WindowId {
        &self.window
    }

    pub const fn renderer(&self) -> &'static str {
        self.renderer
    }

    pub const fn stage(&self) -> RenderErrorStage {
        self.stage
    }

    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    pub const fn code(&self) -> i32 {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "renderer '{}' failed to {} window '{}' during {} (0x{:08X}): {}",
            self.renderer,
            self.operation,
            self.window.as_str(),
            self.stage.as_str(),
            self.code as u32,
            self.message
        )
    }
}

impl std::error::Error for RenderError {}
