#[test]
#[cfg_attr(miri, ignore)]
fn compile_fail_full() {
    let t = trybuild::TestCases::new();

    #[cfg(feature = "full")]
    t.pass("tests/pass/forward_args_and_output.rs");

    #[cfg(feature = "full")]
    t.pass("tests/pass/macros_main_return.rs");

    #[cfg(feature = "full")]
    t.pass("tests/pass/macros_main_loop.rs");

    #[cfg(feature = "full")]
    t.compile_fail("tests/fail/macros_invalid_input.rs");

    #[cfg(feature = "full")]
    t.compile_fail("tests/fail/macros_dead_code.rs");

    #[cfg(feature = "full")]
    t.compile_fail("tests/fail/macros_join.rs");

    #[cfg(feature = "full")]
    t.compile_fail("tests/fail/macros_try_join.rs");

    #[cfg(feature = "full")]
    t.compile_fail("tests/fail/macros_type_mismatch.rs");

    #[cfg(all(feature = "rt", not(feature = "full")))]
    t.compile_fail("tests/fail/macros_core_no_default.rs");

    drop(t);
}
