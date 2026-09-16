use crate::resources::Resources;

/// A typed, application-scoped data store.
///
/// Mutations are observed at the `StoreRuntime::update` boundary. Store methods therefore return
/// only business values and never describe UI invalidation.
pub trait StoreUnit: Send + Sync + 'static {
    const KEY: &'static str;

    fn create(resources: &Resources) -> Self;

    fn unit_name() -> &'static str {
        Self::KEY
    }
}
