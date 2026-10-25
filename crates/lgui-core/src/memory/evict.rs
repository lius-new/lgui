//! Eviction interface shared by framework-owned caches.
//!
//! The framework owns cache storage and accounting; the application owns the
//! decision of what to keep. A cache describes its entries through
//! [`CacheView`] and reports events to the application's [`CacheObserver`],
//! which may evict through the same view. No eviction policy lives here: a
//! cache without an observer keeps whatever its own budget allows.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use super::{CachePriority, CacheUsage, RetentionClass};

/// Stable handle of one cache entry, unique within its cache.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntryId(pub u64);

/// What a cache knows about one entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryInfo {
    pub id: EntryId,
    pub bytes: usize,
    /// Cache generation (frame count) in which the entry was last used.
    pub last_used: u64,
    pub retention: RetentionClass,
    pub priority: CachePriority,
}

/// How far a cache that cannot enumerate its contents should shrink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShrinkLevel {
    /// Drop what recent frames did not use.
    Moderate,
    /// Drop everything that can be rebuilt.
    Critical,
}

/// Why an observer is being called.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheEvent {
    /// An entry was stored. The entry may already have been evicted by the
    /// cache's own budget.
    Inserted(EntryId),
    /// A frame finished drawing.
    FrameEnd,
    /// The application asked for a pass through [`CacheHooks::request_pass`].
    Requested,
}

/// A cache as seen by an observer for the duration of one callback.
pub trait CacheView {
    fn name(&self) -> &'static str;
    fn usage(&self) -> CacheUsage;
    /// The cache's current generation, comparable with [`EntryInfo::last_used`].
    fn generation(&self) -> u64;
    /// Calls `visit` for every entry that can be evicted by id.
    fn visit_entries(&self, visit: &mut dyn FnMut(&EntryInfo));
    /// Evicts the given entries and returns the bytes released. Unknown ids
    /// are ignored.
    fn evict(&mut self, ids: &[EntryId]) -> usize;
    /// Shrinks storage that is not enumerable (for example font data) and
    /// whatever else the level covers. Returns the bytes released when known.
    fn shrink(&mut self, level: ShrinkLevel) -> usize;
}

/// Application hook called by caches at the events of [`CacheEvent`].
///
/// Callbacks run on the rendering thread inside the draw path, so they must
/// be quick: look at entries, evict a few, return.
pub trait CacheObserver: Send + Sync + 'static {
    fn on_event(&self, event: CacheEvent, cache: &mut dyn CacheView);
}

/// Shared connection between an application and its caches. Cloning is cheap;
/// every clone refers to the same observer and request counter.
#[derive(Clone, Default)]
pub struct CacheHooks {
    inner: Arc<HooksInner>,
}

#[derive(Default)]
struct HooksInner {
    observer: Mutex<Option<Arc<dyn CacheObserver>>>,
    requested: AtomicU64,
}

impl CacheHooks {
    /// Installs or removes the application's observer.
    pub fn set_observer(&self, observer: Option<Arc<dyn CacheObserver>>) {
        *self.inner.observer.lock().expect("cache observer poisoned") = observer;
    }

    pub fn observer(&self) -> Option<Arc<dyn CacheObserver>> {
        self.inner
            .observer
            .lock()
            .expect("cache observer poisoned")
            .clone()
    }

    /// Asks every cache to call the observer with [`CacheEvent::Requested`]
    /// at its next frame boundary. Cheap enough to call from any thread.
    pub fn request_pass(&self) {
        self.inner.requested.fetch_add(1, Ordering::AcqRel);
    }

    /// Counter a cache compares against to notice new requests.
    #[doc(hidden)]
    pub fn requested(&self) -> u64 {
        self.inner.requested.load(Ordering::Acquire)
    }
}
