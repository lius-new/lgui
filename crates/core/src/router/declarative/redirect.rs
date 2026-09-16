use crate::{core::group, router::Location};

use super::{route, Route};

/// Defines an absolute replace redirect. Redirects run after a committed render.
pub fn redirect(pattern: &'static str, destination: &'static str) -> Route<Location> {
    assert!(
        destination.starts_with('/'),
        "redirect destinations must be absolute"
    );
    let destination = Location::new(destination);
    route(pattern, move |cx| {
        let current = cx.use_route::<Location>();
        let replace = cx.use_replace::<Location>();
        let destination = destination.clone();
        let effect_destination = destination.clone();
        cx.use_effect((current.clone(), destination), move || {
            if current != effect_destination {
                replace(effect_destination);
            }
        });
        group(cx.viewport())
    })
}
