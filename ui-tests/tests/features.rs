#[test]
fn features() {
    let t = trybuild::TestCases::new();

    #[cfg(feature = "executor-without-current-thread")]
    t.compile_fail("tests/ui/executor_without_current_thread.rs");

    drop(t);
}
