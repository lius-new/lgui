use super::*;
#[test]
fn content_flattens_tuples_options_vectors_and_fragments() {
    let mut children = Vec::new();
    (
        "first",
        Some(2_u32),
        vec![content_text("third"), content_text("fourth")],
        fragment(("fifth", None::<Element>)),
    )
        .append_to(&mut children);

    assert_eq!(children.len(), 5);
}

#[test]
fn explicit_keys_do_not_depend_on_list_position() {
    let first = content_text("row").key("stable");
    let second = content_text("row").key("stable");

    assert_eq!(first.key.segment(0), second.key.segment(99));
}
