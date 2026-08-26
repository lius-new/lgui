use std::any::Any;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiEventFlags {
    pub consumed: bool,
    pub changed: bool,
    pub route_changed: bool,
    pub needs_frame: bool,
    pub default_prevented: bool,
}

/// Backend- and application-independent context passed to declarative event handlers.
///
/// Application crates add business behavior through extension traits. The core only owns
/// propagation, scheduling flags and an opaque host-command slot.
pub struct UiEventContext {
    application: &'static (dyn Any + Send + Sync),
    flags: UiEventFlags,
    propagation_stopped: bool,
    host_command: Option<Box<dyn Any + Send>>,
}

#[derive(Clone, Copy)]
pub struct UiAsyncContext;

impl UiEventContext {
    pub fn new(application: &'static (dyn Any + Send + Sync)) -> Self {
        Self {
            application,
            flags: UiEventFlags::default(),
            propagation_stopped: false,
            host_command: None,
        }
    }

    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
        self.flags.consumed = true;
    }

    pub fn prevent_default(&mut self) {
        self.flags.default_prevented = true;
        self.flags.consumed = true;
    }

    pub fn propagation_stopped(&self) -> bool {
        self.propagation_stopped
    }

    pub fn default_prevented(&self) -> bool {
        self.flags.default_prevented
    }

    pub fn flags(&self) -> UiEventFlags {
        self.flags
    }

    pub fn application<T: Any + Send + Sync>(&self) -> &'static T {
        self.application.downcast_ref::<T>().unwrap_or_else(|| {
            panic!(
                "event context application type mismatch: expected `{}`",
                std::any::type_name::<T>()
            )
        })
    }

    pub fn mark_consumed(&mut self) {
        self.flags.consumed = true;
    }

    pub fn mark_changed(&mut self, route_changed: bool) {
        self.flags.consumed = true;
        self.flags.changed = true;
        self.flags.route_changed |= route_changed;
        self.flags.needs_frame = true;
    }

    pub fn request_frame(&mut self) {
        self.flags.needs_frame = true;
    }

    pub fn set_host_command<T: Any + Send>(&mut self, command: T) {
        self.flags.consumed = true;
        self.host_command = Some(Box::new(command));
    }

    pub fn take_host_command<T: Any + Send>(&mut self) -> Option<T> {
        self.host_command
            .take()
            .map(|command| {
                command.downcast::<T>().unwrap_or_else(|_| {
                    panic!(
                        "event context host command type mismatch: expected `{}`",
                        std::any::type_name::<T>()
                    )
                })
            })
            .map(|command| *command)
    }
}
