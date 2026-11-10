cfg_net! {
    use crate::net::addr::{self, ToSocketAddrs};

    use std::io;
    use std::net::SocketAddr;

    /// Performs a DNS resolution.
    ///
    /// The returned iterator may not actually yield any values depending on the
    /// outcome of any resolution performed.
    ///
    /// This API is not intended to cover all DNS use cases. Anything beyond the
    /// basic use case should be done with a specialized library.
    ///
    /// # Cancellation
    ///
    /// Resolving a host name runs on the blocking thread pool. Dropping the
    /// returned future stops waiting for the result, but the underlying lookup
    /// continues until it finishes. That unfinished work can delay runtime
    /// shutdown.
    ///
    /// # Examples
    ///
    /// To resolve a DNS entry:
    ///
    /// ```no_run
    /// use tokio::net;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     for addr in net::lookup_host("localhost:3000").await? {
    ///         println!("socket address is {}", addr);
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn lookup_host<T>(host: T) -> io::Result<impl Iterator<Item = SocketAddr>>
    where
        T: ToSocketAddrs
    {
        addr::to_socket_addrs(host).await
    }
}
