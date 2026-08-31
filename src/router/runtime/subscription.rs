use std::sync::Arc;

use super::RouteChange;

pub(super) type RouteListener<R> = Arc<dyn Fn(&RouteChange<R>) + Send + Sync + 'static>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RouteSubscriptionToken {
    pub(super) router_id: u64,
    pub(super) subscription_id: u64,
}
