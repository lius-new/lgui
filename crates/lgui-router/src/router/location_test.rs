use super::*;

#[test]
fn location_keeps_complete_navigation_state() {
    let location = Location::new("community//articles/42/?tab=comments#reply");
    assert_eq!(location.path(), "/community/articles/42");
    assert_eq!(location.query(), Some("tab=comments"));
    assert_eq!(location.fragment(), Some("reply"));
    assert_eq!(location.href(), "/community/articles/42?tab=comments#reply");
}

#[test]
fn location_resolves_absolute_url_style_and_fragment_targets() {
    let location = Location::new("/community/articles/42?tab=comments");
    assert_eq!(
        location.resolve("../events?page=2").href(),
        "/community/events?page=2"
    );
    assert_eq!(
        location.resolve("#reply").href(),
        "/community/articles/42?tab=comments#reply"
    );
    assert_eq!(location.resolve("/store").href(), "/store");
}

#[test]
fn percent_decode_rejects_invalid_or_non_utf8_values() {
    assert_eq!(percent_decode("hello%20world"), Some("hello world".into()));
    assert_eq!(percent_decode("%GG"), None);
    assert_eq!(percent_decode("%FF"), None);
}

#[test]
fn dynamic_path_segments_round_trip_without_changing_hierarchy() {
    let value = "文章/42 + Rust";
    let encoded = Location::encode_path_segment(value);
    assert_eq!(encoded, "%E6%96%87%E7%AB%A0%2F42%20%2B%20Rust");
    assert_eq!(percent_decode(&encoded).as_deref(), Some(value));
}
