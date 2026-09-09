use std::sync::{Arc, Mutex};

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    Home,
    Settings,
    Profile,
}

#[test]
fn navigate_replace_and_back_preserve_history_semantics() {
    let router = Router::new(Route::Home);

    assert!(router.navigate(Route::Home).is_none());
    assert_eq!(
        router.navigate(Route::Settings).unwrap().previous,
        Route::Home
    );
    assert_eq!(
        router.replace(Route::Profile).unwrap().previous,
        Route::Settings
    );
    assert_eq!(router.current(), Route::Profile);
    assert!(router.snapshot().can_back());

    let change = router.back().expect("back navigation");
    assert_eq!(change.action, RouteAction::Back);
    assert_eq!(change.previous, Route::Profile);
    assert_eq!(change.current, Route::Home);
    assert!(!router.snapshot().can_back());
    assert!(router.back().is_none());
}

#[test]
fn replace_collapses_history_equal_to_the_new_current_route() {
    let router = Router::new(Route::Home);

    router.navigate(Route::Settings);
    router.replace(Route::Home);

    assert_eq!(router.current(), Route::Home);
    assert!(!router.snapshot().can_back());
    assert!(router.back().is_none());
}

#[test]
fn subscriptions_receive_only_visible_route_changes() {
    let router = Router::new(Route::Home);
    let changes = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&changes);
    let token = router.subscribe(move |change| {
        observed.lock().unwrap().push(change.clone());
    });

    assert!(router.navigate(Route::Home).is_none());
    router.navigate(Route::Settings);
    router.replace(Route::Profile);
    assert_eq!(changes.lock().unwrap().len(), 2);

    assert!(router.unsubscribe(token));
    router.back();
    assert_eq!(changes.lock().unwrap().len(), 2);
}

#[test]
fn subscription_tokens_are_scoped_to_their_router() {
    let first = Router::new(Route::Home);
    let second = Router::new(Route::Home);
    let token = first.subscribe(|_| {});

    assert!(!second.unsubscribe(token.clone()));
    assert!(first.unsubscribe(token));
}

#[test]
fn location_history_preserves_dynamic_query_and_fragment_state() {
    let router = Router::new(crate::router::Location::new("/community"));
    router.navigate(crate::router::Location::new(
        "/community/articles/42?tab=comments#reply",
    ));
    router.navigate(crate::router::Location::new("/store/items/7"));

    assert_eq!(
        router.back().unwrap().current.href(),
        "/community/articles/42?tab=comments#reply"
    );
    assert_eq!(router.back().unwrap().current.path(), "/community");
}
