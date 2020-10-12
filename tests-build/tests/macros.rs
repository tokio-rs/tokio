#[test]
fn compile_fail_full() {
    let t = trybuild::TestCases::new();

    #[cfg(feature = "full")]
    t.compile_fail("tests/fail/macros_invalid_input.rs");

    #[cfg(feature = "rt-core")]
    t.compile_fail("tests/fail/macros_core_no_default.rs");

    drop(t);
}
