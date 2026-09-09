use super::*;

#[test]
fn reactive_changes_do_not_implicitly_request_a_global_frame() {
    let mut context = UiEventContext::new(
        ApplicationContext::empty(crate::memory::test_memory_options()),
        WindowId::new("reactive-change"),
    );

    context.mark_changed(true);

    assert!(context.flags().changed);
    assert!(context.flags().route_changed);
    assert!(!context.flags().needs_frame);

    context.request_frame();
    assert!(context.flags().needs_frame);
}
