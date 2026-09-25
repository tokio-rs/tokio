use std::future;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

/// Converts or resolves without blocking to one or more `SocketAddr` values.
///
/// # DNS
///
/// Implementations of `ToSocketAddrs` for string types require a DNS lookup.
///
/// # Calling
///
/// Currently, this trait is only used as an argument to Tokio functions that
/// need to reference a target socket address. To perform a `SocketAddr`
/// conversion directly, use [`lookup_host()`](super::lookup_host()).
///
/// This trait is sealed and is intended to be opaque. The details of the trait
/// will change. Stabilization is pending enhancements to the Rust language.
pub trait ToSocketAddrs: sealed::ToSocketAddrsPriv {}

type ReadyFuture<T> = future::Ready<io::Result<T>>;

cfg_net! {
    pub(crate) fn to_socket_addrs<T>(arg: T) -> T::Future
    where
        T: ToSocketAddrs,
    {
        arg.to_socket_addrs(sealed::Internal)
    }
}

// ===== impl &impl ToSocketAddrs =====

impl<T: ToSocketAddrs + ?Sized> ToSocketAddrs for &T {}

impl<T> sealed::ToSocketAddrsPriv for &T
where
    T: sealed::ToSocketAddrsPriv + ?Sized,
{
    type Iter = T::Iter;
    type Future = T::Future;

    fn to_socket_addrs(&self, _: sealed::Internal) -> Self::Future {
        (**self).to_socket_addrs(sealed::Internal)
    }
}

// ===== impl SocketAddr =====

impl ToSocketAddrs for SocketAddr {}

impl sealed::ToSocketAddrsPriv for SocketAddr {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = ReadyFuture<Self::Iter>;

    fn to_socket_addrs(&self, _: sealed::Internal) -> Self::Future {
        let iter = Some(*self).into_iter();
        future::ready(Ok(iter))
    }
}

// ===== impl SocketAddrV4 =====

impl ToSocketAddrs for SocketAddrV4 {}

impl sealed::ToSocketAddrsPriv for SocketAddrV4 {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = ReadyFuture<Self::Iter>;

    fn to_socket_addrs(&self, _: sealed::Internal) -> Self::Future {
        SocketAddr::V4(*self).to_socket_addrs(sealed::Internal)
    }
}

// ===== impl SocketAddrV6 =====

impl ToSocketAddrs for SocketAddrV6 {}

impl sealed::ToSocketAddrsPriv for SocketAddrV6 {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = ReadyFuture<Self::Iter>;

    fn to_socket_addrs(&self, _: sealed::Internal) -> Self::Future {
        SocketAddr::V6(*self).to_socket_addrs(sealed::Internal)
    }
}

// ===== impl (IpAddr, u16) =====

impl ToSocketAddrs for (IpAddr, u16) {}

impl sealed::ToSocketAddrsPriv for (IpAddr, u16) {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = ReadyFuture<Self::Iter>;

    fn to_socket_addrs(&self, _: sealed::Internal) -> Self::Future {
        let iter = Some(SocketAddr::from(*self)).into_iter();
        future::ready(Ok(iter))
    }
}

// ===== impl (Ipv4Addr, u16) =====

impl ToSocketAddrs for (Ipv4Addr, u16) {}

impl sealed::ToSocketAddrsPriv for (Ipv4Addr, u16) {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = ReadyFuture<Self::Iter>;

    fn to_socket_addrs(&self, _: sealed::Internal) -> Self::Future {
        let (ip, port) = *self;
        SocketAddrV4::new(ip, port).to_socket_addrs(sealed::Internal)
    }
}

// ===== impl (Ipv6Addr, u16) =====

impl ToSocketAddrs for (Ipv6Addr, u16) {}

impl sealed::ToSocketAddrsPriv for (Ipv6Addr, u16) {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = ReadyFuture<Self::Iter>;

    fn to_socket_addrs(&self, _: sealed::Internal) -> Self::Future {
        let (ip, port) = *self;
        SocketAddrV6::new(ip, port, 0, 0).to_socket_addrs(sealed::Internal)
    }
}

// ===== impl &[SocketAddr] =====

impl ToSocketAddrs for &[SocketAddr] {}

impl sealed::ToSocketAddrsPriv for &[SocketAddr] {
    type Iter = std::vec::IntoIter<SocketAddr>;
    type Future = ReadyFuture<Self::Iter>;

    fn to_socket_addrs(&self, _: sealed::Internal) -> Self::Future {
        #[inline]
        fn slice_to_vec(addrs: &[SocketAddr]) -> Vec<SocketAddr> {
            addrs.to_vec()
        }

        // This uses a helper method because clippy doesn't like the `to_vec()`
        // call here (it will allocate, whereas `self.iter().copied()` would
        // not), but it's actually necessary in order to ensure that the
        // returned iterator is valid for the `'static` lifetime, which the
        // borrowed `slice::Iter` iterator would not be.
        //
        // Note that we can't actually add an `allow` attribute for
        // `clippy::unnecessary_to_owned` here, as Tokio's CI runs clippy lints
        // on Rust 1.52 to avoid breaking LTS releases of Tokio. Users of newer
        // Rust versions who see this lint should just ignore it.
        let iter = slice_to_vec(self).into_iter();
        future::ready(Ok(iter))
    }
}

cfg_net! {
    // ===== impl str =====

    impl ToSocketAddrs for str {}

    impl sealed::ToSocketAddrsPriv for str {
        type Iter = sealed::OneOrMore;
        type Future = sealed::MaybeReady;

        fn to_socket_addrs(&self, _: sealed::Internal) -> Self::Future {
            use sealed::MaybeReady;

            // First check if the input parses as a socket address
            let res: Result<SocketAddr, _> = self.parse();

            if let Ok(addr) = res {
                return MaybeReady(sealed::State::Ready(Some(addr)));
            }

            MaybeReady(sealed::lookup_str(self.to_owned()))
        }
    }

    // ===== impl (&str, u16) =====

    impl ToSocketAddrs for (&str, u16) {}

    impl sealed::ToSocketAddrsPriv for (&str, u16) {
        type Iter = sealed::OneOrMore;
        type Future = sealed::MaybeReady;

        fn to_socket_addrs(&self, _: sealed::Internal) -> Self::Future {
            use sealed::MaybeReady;

            let (host, port) = *self;

            // try to parse the host as a regular IP address first
            if let Ok(addr) = host.parse::<Ipv4Addr>() {
                let addr = SocketAddrV4::new(addr, port);
                let addr = SocketAddr::V4(addr);

                return MaybeReady(sealed::State::Ready(Some(addr)));
            }

            if let Ok(addr) = host.parse::<Ipv6Addr>() {
                let addr = SocketAddrV6::new(addr, port, 0, 0);
                let addr = SocketAddr::V6(addr);

                return MaybeReady(sealed::State::Ready(Some(addr)));
            }

            MaybeReady(sealed::lookup_host(host.to_owned(), port))
        }
    }

    // ===== impl (String, u16) =====

    impl ToSocketAddrs for (String, u16) {}

    impl sealed::ToSocketAddrsPriv for (String, u16) {
        type Iter = sealed::OneOrMore;
        type Future = sealed::MaybeReady;

        fn to_socket_addrs(&self, _: sealed::Internal) -> Self::Future {
            (self.0.as_str(), self.1).to_socket_addrs(sealed::Internal)
        }
    }

    // ===== impl String =====

    impl ToSocketAddrs for String {}

    impl sealed::ToSocketAddrsPriv for String {
        type Iter = <str as sealed::ToSocketAddrsPriv>::Iter;
        type Future = <str as sealed::ToSocketAddrsPriv>::Future;

        fn to_socket_addrs(&self, _: sealed::Internal) -> Self::Future {
            self[..].to_socket_addrs(sealed::Internal)
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

    #[doc(hidden)]
    pub trait ToSocketAddrsPriv {
        type Iter: Iterator<Item = SocketAddr> + Send + 'static;
        type Future: Future<Output = io::Result<Self::Iter>> + Send + 'static;

        fn to_socket_addrs(&self, internal: Internal) -> Self::Future;
    }

    #[allow(missing_debug_implementations)]
    pub struct Internal;

    cfg_net! {
        use std::option;
        use std::pin::Pin;
        use std::task::{ready,Context, Poll};
        use std::vec;

        #[doc(hidden)]
        #[derive(Debug)]
        pub struct MaybeReady(pub(super) State);

        #[derive(Debug)]
        pub(super) enum State {
            Ready(Option<SocketAddr>),
            Lookup(Lookup),
        }

        // A hostname lookup in flight: `std`'s resolver on the blocking pool,
        // or on emscripten, whose synchronous `getaddrinfo` has nothing to
        // block on, its asynchronous one awaited through the I/O driver.
        #[cfg(not(target_os = "emscripten"))]
        pub(super) type Lookup = crate::blocking::JoinHandle<io::Result<vec::IntoIter<SocketAddr>>>;
        #[cfg(target_os = "emscripten")]
        pub(super) struct Lookup(Pin<Box<dyn Future<Output = io::Result<vec::IntoIter<SocketAddr>>> + Send>>);

        #[cfg(target_os = "emscripten")]
        impl std::fmt::Debug for Lookup {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("Lookup")
            }
        }

        /// Resolves `host:port` as `std::net::ToSocketAddrs` does for a string.
        #[cfg(not(target_os = "emscripten"))]
        pub(super) fn lookup_str(s: String) -> State {
            State::Lookup(crate::blocking::spawn_blocking(move || {
                std::net::ToSocketAddrs::to_socket_addrs(&s)
            }))
        }

        #[cfg(target_os = "emscripten")]
        pub(super) fn lookup_str(s: String) -> State {
            match s.rsplit_once(':').and_then(|(h, p)| Some((h.to_owned(), p.parse().ok()?))) {
                Some((host, port)) => lookup_host(host, port),
                None => State::Lookup(Lookup(Box::pin(std::future::ready(Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid socket address",
                )))))),
            }
        }

        #[cfg(not(target_os = "emscripten"))]
        pub(super) fn lookup_host(host: String, port: u16) -> State {
            State::Lookup(crate::blocking::spawn_blocking(move || {
                std::net::ToSocketAddrs::to_socket_addrs(&(&host[..], port))
            }))
        }

        #[cfg(target_os = "emscripten")]
        pub(super) fn lookup_host(host: String, port: u16) -> State {
            State::Lookup(Lookup(Box::pin(crate::net::emscripten_dns::resolve(host, port))))
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
                match self.0 {
                    State::Ready(ref mut i) => {
                        let iter = OneOrMore::One(i.take().into_iter());
                        Poll::Ready(Ok(iter))
                    }
                    #[cfg(not(target_os = "emscripten"))]
                    State::Lookup(ref mut rx) => {
                        let res = ready!(Pin::new(rx).poll(cx))?.map(OneOrMore::More);

                        Poll::Ready(res)
                    }
                    #[cfg(target_os = "emscripten")]
                    State::Lookup(ref mut fut) => {
                        let res = ready!(fut.0.as_mut().poll(cx)).map(OneOrMore::More);

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
