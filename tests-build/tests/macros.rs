#[test]
fn compile_fail_full() {
    let t = trybuild::TestCases::new();

    #[cfg(feature = "full")]
    t.compile_fail("tests/fail/macros_invalid_input.rs");

    #[cfg(all(feature = "rt-core", not(feature = "full")))]
    t.compile_fail("tests/fail/macros_core_no_default.rs");

    drop(t);
}
