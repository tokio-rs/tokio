#[test]
fn compile_fail() {
    let t = trybuild::TestCases::new();

    #[cfg(feature = "full")]
    t.compile_fail("tests/fail/macros_invalid_input.rs");

    drop(t);
}
