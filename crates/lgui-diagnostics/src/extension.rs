use lgui_core::application::Application;

use crate::{DiagnosticsRegistration, DiagnosticsSink};

pub trait DiagnosticsApplicationExt: Sized {
    fn diagnostics_sink(self, sink: impl DiagnosticsSink + 'static) -> Self;
}

impl<B> DiagnosticsApplicationExt for Application<B> {
    fn diagnostics_sink(self, sink: impl DiagnosticsSink + 'static) -> Self {
        self.provide(DiagnosticsRegistration::new(sink))
    }
}
