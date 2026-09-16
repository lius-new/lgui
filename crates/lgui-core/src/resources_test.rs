use super::*;

#[test]
fn resources_are_typed_and_application_scoped() {
    let first = Resources::new();
    let second = Resources::new();
    first.provide(String::from("first"));
    second.provide(String::from("second"));

    assert_eq!(first.require::<String>().as_str(), "first");
    assert_eq!(second.require::<String>().as_str(), "second");
}
