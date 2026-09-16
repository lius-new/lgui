use super::*;

#[test]
fn options_builder_preserves_filters() {
    let options = FileDialogOptions::new()
        .title("Open image")
        .filter("Images", ["png", "jpg"]);
    assert_eq!(options.title.as_deref(), Some("Open image"));
    assert_eq!(options.filters[0].extensions, ["png", "jpg"]);
}
