#[test]
#[cfg(feature = "tokio-with-net")]
#[allow(unused_imports)]
fn tokio_with_net() {
    // Reactor is present
    use ui_tests::tokio::reactor;

    // net is present
    use ui_tests::tokio::net;
}
#[test]
fn compile_fail() {
    let t = trybuild::TestCases::new();

    #[cfg(feature = "executor-without-current-thread")]
    t.compile_fail("tests/ui/executor_without_current_thread.rs");

    #[cfg(feature = "tokio-no-features")]
    t.compile_fail("tests/ui/tokio_without_net_missing_reactor.rs");

    drop(t);
}
