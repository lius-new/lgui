use super::*;

#[test]
fn opener_rejects_non_external_schemes() {
    assert!(validate_external_url("file:///private.txt").is_err());
    assert!(validate_external_url("https://example.com").is_ok());
}
