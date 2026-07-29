//! WP-02 wiring only. Proves `cargo test` runs a test in `src-tauri`; it does
//! not exercise any production code, which is out of scope for this package.

#[test]
fn cargo_test_runs() {
    assert_eq!(2 + 2, 4);
}
