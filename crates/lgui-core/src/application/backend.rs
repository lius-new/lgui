use super::{AppView, ApplicationContext, WindowOptions};

pub trait ApplicationBackend: Sized {
    type Error;

    fn run(
        self,
        options: WindowOptions,
        view: AppView,
        context: ApplicationContext,
    ) -> Result<(), Self::Error>;
}
