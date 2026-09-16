use std::{
    borrow::Borrow,
    fmt,
    future::Future,
    hash::{Hash, Hasher},
    marker::PhantomData,
    pin::Pin,
    sync::Arc,
};

use crate::core::UiAsyncContext;

const MAX_EVENT_KEY_BYTES: usize = 128;

/// A stable routing key whose payload type is checked by Rust.
///
/// Keys are compared by name. An Application rejects attempts to subscribe to
/// the same name with different payload types.
pub struct EventKey<T> {
    name: EventKeyName,
    _payload: PhantomData<fn(T)>,
}

enum EventKeyName {
    Static(&'static str),
    Shared(Arc<str>),
}

impl<T> EventKey<T> {
    /// Creates a static Event key.
    ///
    /// A key must be non-empty, at most 128 bytes, and contain only lowercase
    /// ASCII letters, digits, `.`, `_`, `-`, or `:`.
    pub const fn new(name: &'static str) -> Self {
        assert_valid_static_key(name);
        Self {
            name: EventKeyName::Static(name),
            _payload: PhantomData,
        }
    }

    /// Creates a runtime Event key, for example from a server-provided event
    /// name. Invalid names are rejected before they reach the Event bus.
    pub fn dynamic(name: impl Into<Arc<str>>) -> Result<Self, InvalidEventKey> {
        let name = name.into();
        validate_key(&name)?;
        Ok(Self {
            name: EventKeyName::Shared(name),
            _payload: PhantomData,
        })
    }

    pub fn as_str(&self) -> &str {
        match &self.name {
            EventKeyName::Static(name) => name,
            EventKeyName::Shared(name) => name,
        }
    }

    pub(crate) fn shared_name(&self) -> Arc<str> {
        match &self.name {
            EventKeyName::Static(name) => Arc::from(*name),
            EventKeyName::Shared(name) => Arc::clone(name),
        }
    }
}

impl<T> Clone for EventKey<T> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            _payload: PhantomData,
        }
    }
}

impl Clone for EventKeyName {
    fn clone(&self) -> Self {
        match self {
            Self::Static(name) => Self::Static(name),
            Self::Shared(name) => Self::Shared(Arc::clone(name)),
        }
    }
}

impl<T> fmt::Debug for EventKey<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("EventKey")
            .field(&self.as_str())
            .finish()
    }
}

impl<T> fmt::Display for EventKey<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl<T> PartialEq for EventKey<T> {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<T> Eq for EventKey<T> {}

impl<T> Hash for EventKey<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl<T> Borrow<str> for EventKey<T> {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidEventKey {
    name: Arc<str>,
}

impl InvalidEventKey {
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for InvalidEventKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid event key `{}`; expected 1..={MAX_EVENT_KEY_BYTES} bytes containing only lowercase ASCII letters, digits, `.`, `_`, `-`, or `:`",
            self.name
        )
    }
}

impl std::error::Error for InvalidEventKey {}

const fn assert_valid_static_key(name: &str) {
    let bytes = name.as_bytes();
    assert!(!bytes.is_empty(), "event key must not be empty");
    assert!(
        bytes.len() <= MAX_EVENT_KEY_BYTES,
        "event key exceeds 128 bytes"
    );
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        assert!(
            matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' | b':'),
            "event key contains an invalid byte"
        );
        index += 1;
    }
}

fn validate_key(name: &str) -> Result<(), InvalidEventKey> {
    let valid = !name.is_empty()
        && name.len() <= MAX_EVENT_KEY_BYTES
        && name
            .bytes()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' | b':'));
    if valid {
        Ok(())
    } else {
        Err(InvalidEventKey {
            name: Arc::from(name),
        })
    }
}

/// A typed, Application-scoped event.
///
/// This compatibility contract routes through `EventKey::new(NAME)`. New code
/// should declare an `EventKey<T>` directly.
pub trait Event: Clone + Send + Sync + 'static {
    const NAME: &'static str;
}

pub type EventFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

pub trait AsyncEventHandler<E>: Send + Sync + 'static
where
    E: Event,
{
    fn call(&self, context: UiAsyncContext, event: E) -> EventFuture;
}

impl<E, F, Fut> AsyncEventHandler<E> for F
where
    E: Event,
    F: Fn(UiAsyncContext, E) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    fn call(&self, context: UiAsyncContext, event: E) -> EventFuture {
        Box::pin((self)(context, event))
    }
}
