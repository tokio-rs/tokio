#![allow(unused_imports)]

#[test]
#[cfg(feature = "tokio-net")]
fn net_default() {
    use ui_tests::tokio_net::driver::{set_default, Handle, Reactor, Registration};
    use ui_tests::tokio_net::util::PollEvented;
}

#[test]
#[cfg(feature = "net-with-tcp")]
fn net_with_tcp() {
    use ui_tests::tokio_net::tcp;
}

#[test]
#[cfg(feature = "net-with-udp")]
fn net_with_udp() {
    use ui_tests::tokio_net::udp;
}

#[test]
#[cfg(feature = "tokio-with-net")]
fn tokio_with_net() {
    // net is present
    use ui_tests::tokio::net;
}
#[test]
fn compile_fail() {
    let t = trybuild::TestCases::new();

    #[cfg(feature = "executor-without-current-thread")]
    t.compile_fail("tests/ui/executor_without_current_thread.rs");

    #[cfg(feature = "net-no-features")]
    {
        t.compile_fail("tests/ui/net_without_tcp_missing_tcp.rs");
        t.compile_fail("tests/ui/net_without_udp_missing_udp.rs");
    }

    #[cfg(feature = "tokio-no-features")]
    t.compile_fail("tests/ui/tokio_without_net_missing_net.rs");

    drop(t);
}
