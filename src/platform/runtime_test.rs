use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

#[test]
fn wake_handle_is_cloneable_and_backend_neutral() {
    let count = Arc::new(AtomicUsize::new(0));
    let wake = WakeHandle::new({
        let count = Arc::clone(&count);
        move || {
            count.fetch_add(1, Ordering::Relaxed);
        }
    });

    wake.clone().wake();
    (wake.as_ui_wake())();

    assert_eq!(count.load(Ordering::Relaxed), 2);
}
