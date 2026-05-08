// Trivial canary that the JSON subscriber compiles and the version env var is set.
#[test]
fn version_is_set() {
    assert!(!env!("CARGO_PKG_VERSION").is_empty());
}
