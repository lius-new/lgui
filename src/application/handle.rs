use std::sync::Arc;

pub type ApplicationTask = Box<dyn FnOnce() + Send + 'static>;

/// Cloneable, platform-neutral access to the running application event loop.
#[derive(Clone)]
pub struct ApplicationHandle {
    post: Arc<dyn Fn(ApplicationTask) + Send + Sync>,
    request_frame: Arc<dyn Fn() + Send + Sync>,
}

impl ApplicationHandle {
    pub fn new(
        post: impl Fn(ApplicationTask) + Send + Sync + 'static,
        request_frame: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            post: Arc::new(post),
            request_frame: Arc::new(request_frame),
        }
    }

    pub fn post(&self, task: impl FnOnce() + Send + 'static) {
        (self.post)(Box::new(task));
    }

    pub fn request_frame(&self) {
        (self.request_frame)();
    }
}
