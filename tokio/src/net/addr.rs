use std::io;
use std::net::SocketAddr;

/// Converts or resolves without blocking to one or more `SocketAddr` values.
///
/// # DNS
///
/// Implementations of `ToSocketAddrs` for string types require a DNS lookup.
/// These implementations are only provided when Tokio is used with the
/// **`net`** feature flag.
///
/// # Calling
///
/// Currently, this trait is only used as an argument to Tokio functions that
/// need to reference a target socket address. To perform a `SocketAddr`
/// conversion directly, use [`lookup_host()`](super::lookup_host()).
///
/// This trait is sealed and is intended to be opaque. The details of the trait
/// will change. Stabilization is pending enhancements to the Rust language.
pub use t10::net::ToSocketAddrs;

cfg_net! {
    pub(crate) async fn to_socket_addrs<T>(arg: T) -> io::Result<impl Iterator<Item = SocketAddr>>
    where
        T: ToSocketAddrs,
    {
        t10::net::lookup_host(arg).await
    }
}
