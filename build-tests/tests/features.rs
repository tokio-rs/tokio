#![allow(unused_imports)]

#[test]
#[cfg(feature = "tokio-net")]
fn net_default() {
    use build_tests::tokio_net::driver::{set_default, Handle, Reactor, Registration};
    use build_tests::tokio_net::util::PollEvented;
}

#[test]
#[cfg(feature = "net-with-tcp")]
fn net_with_tcp() {
    use build_tests::tokio_net::tcp;
}

#[test]
#[cfg(feature = "net-with-udp")]
fn net_with_udp() {
    use build_tests::tokio_net::udp;
}

#[test]
#[cfg(feature = "net-with-uds")]
fn net_with_udp() {
    use build_tests::tokio_net::uds;
}

#[test]
#[cfg(feature = "tokio-with-net")]
fn tokio_with_net() {
    // net is present
    use build_tests::tokio::net;
}
#[test]
fn compile_fail() {
    let t = trybuild::TestCases::new();

    #[cfg(feature = "executor-without-current-thread")]
    t.compile_fail("tests/fail/executor_without_current_thread.rs");

    #[cfg(feature = "net-no-features")]
    {
        t.compile_fail("tests/fail/net_without_tcp_missing_tcp.rs");
        t.compile_fail("tests/fail/net_without_udp_missing_udp.rs");
        t.compile_fail("tests/fail/net_without_uds_missing_uds.rs");
    }

    #[cfg(feature = "tokio-no-features")]
    t.compile_fail("tests/fail/tokio_without_net_missing_net.rs");

    drop(t);
}
