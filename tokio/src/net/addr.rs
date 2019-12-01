use crate::future;

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

/// Convert or resolve without blocking to one or more `SocketAddr` values.
///
/// # DNS
///
/// Implementations of `ToSocketAddrs` for string types require a DNS lookup.
/// These implementations are only provided when Tokio is used with the
/// **`dns`** feature flag.
///
/// # Calling
///
/// Currently, this trait is only used as an argument to Tokio functions that
/// need to reference a target socket address.
///
/// This trait is sealed and is intended to be opaque. Users of Tokio should
/// only use `ToSocketAddrs` in trait bounds and __must not__ attempt to call
/// the functions directly or reference associated types. Changing these is not
/// considered a breaking change.
pub trait ToSocketAddrs: sealed::ToSocketAddrsPriv {}

type ReadyFuture<T> = future::Ready<io::Result<T>>;

// ===== impl &impl ToSocketAddrs =====

impl<T: ToSocketAddrs + ?Sized> ToSocketAddrs for &T {}

impl<T> sealed::ToSocketAddrsPriv for &T
where
    T: sealed::ToSocketAddrsPriv + ?Sized,
{
    type Iter = T::Iter;
    type Future = T::Future;

    fn to_socket_addrs(&self) -> Self::Future {
        (**self).to_socket_addrs()
    }
}

cfg_dns! {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    /// LookupHost contains resolved SocketAddrs.
    ///
    /// This type is created by the function [`lookup_host`]. See its documentation for more.
    #[derive(Debug)]
    pub struct LookupHost<T>
    where
        T: sealed::ToSocketAddrsPriv,
        T: ToSocketAddrs<Future = sealed::MaybeReady>,
        T: ToSocketAddrs<Iter = sealed::OneOrMore>,
    {
        inner: LookupHostInner<T>,
    }

    impl<T> Future for LookupHost<T>
    where
        T: sealed::ToSocketAddrsPriv,
        T: ToSocketAddrs<Future = sealed::MaybeReady>,
        T: ToSocketAddrs<Iter = sealed::OneOrMore>,
    {
        type Output = io::Result<SocketAddr>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if let LookupHostInner::Pending { fut } = &mut self.inner {
                if let Poll::Ready(addrs) = Pin::new(fut).poll(cx) {
                    match addrs {
                        Ok(mut addrs) => {
                            let addr = addrs.next().unwrap();
                            return Poll::Ready(Ok(addr))
                        },
                        Err(e) => return Poll::Ready(Err(e))
                    };
                } else {
                    Poll::Pending
                }
            } else {
                unimplemented!()
            }
        }
    }

    // TODO: collapse this into a single enum once Rust 1.40 ships
    // with the the `#[non_exhaustive]` attribute if Tokio's MSRV allows
    //
    // https://github.com/rust-lang/rust/pull/64639
    #[derive(Debug)]
    enum LookupHostInner<T>
    where
        T: sealed::ToSocketAddrsPriv,
        T: ToSocketAddrs<Future = sealed::MaybeReady>,
        T: ToSocketAddrs<Iter = sealed::OneOrMore>,
    {
        Pending {
            fut: <T as sealed::ToSocketAddrsPriv>::Future,
        },
        Addrs {
            addrs: sealed::OneOrMore,
        },
    }

    impl<T> LookupHost<T>
    where
        T: sealed::ToSocketAddrsPriv + Unpin,
        T: ToSocketAddrs<Future = sealed::MaybeReady>,
        T: ToSocketAddrs<Iter = sealed::OneOrMore>,
    {
        /// Fetches the next resolved address. If the first
        pub async fn next_addr(&mut self) -> io::Result<Option<SocketAddr>> {
            let inner = &mut self.inner;
            if let LookupHostInner::Pending { fut } = inner {
                *inner = LookupHostInner::Addrs { addrs: fut.await? };
            }

            match inner {
                LookupHostInner::Addrs { addrs } => Ok(addrs.next()),
                _ => unreachable!("The future should have already been resolved"),
            }
        }
    }

    /// Performs a DNS resolution.
    ///
    /// This API is not intended to cover all DNS use cases.
    /// Anything beyond the basic use case should be done with a specialized library.
    /// There are two, mutually exclusive ways of using this API:
    /// 1. `.await`ing `LookupHost`. This provides _only_ the first resolved DNS entry.
    ///    If no DNS entries are found, the `LookupHost` future will return a
    ///   `std::io::Error`.
    /// 2. Calling `LookupHost::next_addr` to fetch all resolved `SocketAddr`s.
    ///    Once streams are stabilized in the standard library, `LookupHost::next_addr`
    ///    will be replaced a stream.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net;
    ///
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///    let addr = net::lookup_host("localhost:3000").await?;
    ///    println!("The IP address is: {}", addr);
    ///
    ///    Ok(())
    /// }
    /// ```
    pub fn lookup_host<T: ToSocketAddrs>(host: T) -> LookupHost<T>
    where
        T: ToSocketAddrs<Future = sealed::MaybeReady>,
        T: ToSocketAddrs<Iter = sealed::OneOrMore>,
    {
        let fut = host.to_socket_addrs();
        LookupHost {
            inner: LookupHostInner::Pending { fut },
        }
    }
}

// ===== impl SocketAddr =====

impl ToSocketAddrs for SocketAddr {}

impl sealed::ToSocketAddrsPriv for SocketAddr {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = ReadyFuture<Self::Iter>;

    fn to_socket_addrs(&self) -> Self::Future {
        let iter = Some(*self).into_iter();
        future::ok(iter)
    }
}

// ===== impl SocketAddrV4 =====

impl ToSocketAddrs for SocketAddrV4 {}

impl sealed::ToSocketAddrsPriv for SocketAddrV4 {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = ReadyFuture<Self::Iter>;

    fn to_socket_addrs(&self) -> Self::Future {
        SocketAddr::V4(*self).to_socket_addrs()
    }
}

// ===== impl SocketAddrV6 =====

impl ToSocketAddrs for SocketAddrV6 {}

impl sealed::ToSocketAddrsPriv for SocketAddrV6 {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = ReadyFuture<Self::Iter>;

    fn to_socket_addrs(&self) -> Self::Future {
        SocketAddr::V6(*self).to_socket_addrs()
    }
}

// ===== impl (IpAddr, u16) =====

impl ToSocketAddrs for (IpAddr, u16) {}

impl sealed::ToSocketAddrsPriv for (IpAddr, u16) {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = ReadyFuture<Self::Iter>;

    fn to_socket_addrs(&self) -> Self::Future {
        let iter = Some(SocketAddr::from(*self)).into_iter();
        future::ok(iter)
    }
}

// ===== impl (Ipv4Addr, u16) =====

impl ToSocketAddrs for (Ipv4Addr, u16) {}

impl sealed::ToSocketAddrsPriv for (Ipv4Addr, u16) {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = ReadyFuture<Self::Iter>;

    fn to_socket_addrs(&self) -> Self::Future {
        let (ip, port) = *self;
        SocketAddrV4::new(ip, port).to_socket_addrs()
    }
}

// ===== impl (Ipv6Addr, u16) =====

impl ToSocketAddrs for (Ipv6Addr, u16) {}

impl sealed::ToSocketAddrsPriv for (Ipv6Addr, u16) {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = ReadyFuture<Self::Iter>;

    fn to_socket_addrs(&self) -> Self::Future {
        let (ip, port) = *self;
        SocketAddrV6::new(ip, port, 0, 0).to_socket_addrs()
    }
}

cfg_dns! {
    // ===== impl str =====

    impl ToSocketAddrs for str {}

    impl sealed::ToSocketAddrsPriv for str {
        type Iter = sealed::OneOrMore;
        type Future = sealed::MaybeReady;

        fn to_socket_addrs(&self) -> Self::Future {
            use crate::runtime::spawn_blocking;
            use sealed::MaybeReady;

            // First check if the input parses as a socket address
            let res: Result<SocketAddr, _> = self.parse();

            if let Ok(addr) = res {
                return MaybeReady::Ready(Some(addr));
            }

            // Run DNS lookup on the blocking pool
            let s = self.to_owned();

            MaybeReady::Blocking(spawn_blocking(move || {
                std::net::ToSocketAddrs::to_socket_addrs(&s)
            }))
        }
    }

    // ===== impl (&str, u16) =====

    impl ToSocketAddrs for (&str, u16) {}

    impl sealed::ToSocketAddrsPriv for (&str, u16) {
        type Iter = sealed::OneOrMore;
        type Future = sealed::MaybeReady;

        fn to_socket_addrs(&self) -> Self::Future {
            use crate::runtime::spawn_blocking;
            use sealed::MaybeReady;

            let (host, port) = *self;

            // try to parse the host as a regular IP address first
            if let Ok(addr) = host.parse::<Ipv4Addr>() {
                let addr = SocketAddrV4::new(addr, port);
                let addr = SocketAddr::V4(addr);

                return MaybeReady::Ready(Some(addr));
            }

            if let Ok(addr) = host.parse::<Ipv6Addr>() {
                let addr = SocketAddrV6::new(addr, port, 0, 0);
                let addr = SocketAddr::V6(addr);

                return MaybeReady::Ready(Some(addr));
            }

            let host = host.to_owned();

            MaybeReady::Blocking(spawn_blocking(move || {
                std::net::ToSocketAddrs::to_socket_addrs(&(&host[..], port))
            }))
        }
    }

    // ===== impl String =====

    impl ToSocketAddrs for String {}

    impl sealed::ToSocketAddrsPriv for String {
        type Iter = <str as sealed::ToSocketAddrsPriv>::Iter;
        type Future = <str as sealed::ToSocketAddrsPriv>::Future;

        fn to_socket_addrs(&self) -> Self::Future {
            (&self[..]).to_socket_addrs()
        }
    }
}

pub(crate) mod sealed {
    //! The contents of this trait are intended to remain private and __not__
    //! part of the `ToSocketAddrs` public API. The details will change over
    //! time.

    use std::future::Future;
    use std::io;
    use std::net::SocketAddr;

    cfg_dns! {
        use crate::task::JoinHandle;

        use std::option;
        use std::pin::Pin;
        use std::task::{Context, Poll};
        use std::vec;
    }

    #[doc(hidden)]
    pub trait ToSocketAddrsPriv {
        type Iter: Iterator<Item = SocketAddr> + Send + 'static;
        type Future: Future<Output = io::Result<Self::Iter>> + Send + 'static;

        fn to_socket_addrs(&self) -> Self::Future;
    }

    cfg_dns! {
        #[doc(hidden)]
        #[derive(Debug)]
        pub enum MaybeReady {
            Ready(Option<SocketAddr>),
            Blocking(JoinHandle<io::Result<vec::IntoIter<SocketAddr>>>),
        }

        #[doc(hidden)]
        #[derive(Debug)]
        pub enum OneOrMore {
            One(option::IntoIter<SocketAddr>),
            More(vec::IntoIter<SocketAddr>),
        }

        impl Future for MaybeReady {
            type Output = io::Result<OneOrMore>;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                match *self {
                    MaybeReady::Ready(ref mut i) => {
                        let iter = OneOrMore::One(i.take().into_iter());
                        Poll::Ready(Ok(iter))
                    }
                    MaybeReady::Blocking(ref mut rx) => {
                        let res = ready!(Pin::new(rx).poll(cx))?.map(OneOrMore::More);

                        Poll::Ready(res)
                    }
                }
            }
        }

        impl Iterator for OneOrMore {
            type Item = SocketAddr;

            fn next(&mut self) -> Option<Self::Item> {
                match self {
                    OneOrMore::One(i) => i.next(),
                    OneOrMore::More(i) => i.next(),
                }
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                match self {
                    OneOrMore::One(i) => i.size_hint(),
                    OneOrMore::More(i) => i.size_hint(),
                }
            }
        }
    }
}
