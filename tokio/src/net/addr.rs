use futures_util::future;
use std::io;
use std::net::{IpAddr, SocketAddr};
#[cfg(feature = "dns")]
use std::net::{Ipv4Addr, Ipv6Addr};

/// Convert or resolve without blocking to one or more `SocketAddr` values.
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

// ===== impl SocketAddr =====

impl ToSocketAddrs for SocketAddr {}

impl sealed::ToSocketAddrsPriv for SocketAddr {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = ReadyFuture<Self::Iter>;

    fn to_socket_addrs(&self) -> Self::Future {
        let iter = Some(*self).into_iter();
        future::ready(Ok(iter))
    }
}

// ===== impl str =====

#[cfg(feature = "dns")]
impl ToSocketAddrs for str {}

#[cfg(feature = "dns")]
impl sealed::ToSocketAddrsPriv for str {
    type Iter = sealed::OneOrMore;
    type Future = sealed::MaybeReady;

    fn to_socket_addrs(&self) -> Self::Future {
        use crate::blocking;
        use sealed::MaybeReady;

        // First check if the input parses as a socket address
        let res: Result<SocketAddr, _> = self.parse();

        if let Ok(addr) = res {
            return MaybeReady::Ready(Some(addr));
        }

        // Run DNS lookup on the blocking pool
        let s = self.to_owned();

        MaybeReady::Blocking(blocking::spawn_blocking(move || {
            std::net::ToSocketAddrs::to_socket_addrs(&s)
        }))
    }
}

// ===== impl (&str, u16) =====

#[cfg(feature = "dns")]
impl ToSocketAddrs for (&'_ str, u16) {}

#[cfg(feature = "dns")]
impl sealed::ToSocketAddrsPriv for (&'_ str, u16) {
    type Iter = sealed::OneOrMore;
    type Future = sealed::MaybeReady;

    fn to_socket_addrs(&self) -> Self::Future {
        use crate::blocking;
        use sealed::MaybeReady;
        use std::net::{SocketAddrV4, SocketAddrV6};

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

        MaybeReady::Blocking(blocking::spawn_blocking(move || {
            std::net::ToSocketAddrs::to_socket_addrs(&(&host[..], port))
        }))
    }
}

// ===== impl (IpAddr, u16) =====

impl ToSocketAddrs for (IpAddr, u16) {}

impl sealed::ToSocketAddrsPriv for (IpAddr, u16) {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = ReadyFuture<Self::Iter>;

    fn to_socket_addrs(&self) -> Self::Future {
        let iter = Some(SocketAddr::from(*self)).into_iter();
        future::ready(Ok(iter))
    }
}

// ===== impl String =====

#[cfg(feature = "dns")]
impl ToSocketAddrs for String {}

#[cfg(feature = "dns")]
impl sealed::ToSocketAddrsPriv for String {
    type Iter = <str as sealed::ToSocketAddrsPriv>::Iter;
    type Future = <str as sealed::ToSocketAddrsPriv>::Future;

    fn to_socket_addrs(&self) -> Self::Future {
        (&self[..]).to_socket_addrs()
    }
}

// ===== impl &'_ impl ToSocketAddrs =====

impl<T: ToSocketAddrs + ?Sized> ToSocketAddrs for &'_ T {}

impl<T> sealed::ToSocketAddrsPriv for &'_ T
where
    T: sealed::ToSocketAddrsPriv + ?Sized,
{
    type Iter = T::Iter;
    type Future = T::Future;

    fn to_socket_addrs(&self) -> Self::Future {
        (**self).to_socket_addrs()
    }
}

pub(crate) mod sealed {
    //! The contents of this trait are intended to remain private and __not__
    //! part of the `ToSocketAddrs` public API. The details will change over
    //! time.

    #[cfg(feature = "dns")]
    use crate::task::JoinHandle;

    use std::future::Future;
    use std::io;
    use std::net::SocketAddr;
    #[cfg(feature = "dns")]
    use std::option;
    #[cfg(feature = "dns")]
    use std::pin::Pin;
    #[cfg(feature = "dns")]
    use std::task::{Context, Poll};
    #[cfg(feature = "dns")]
    use std::vec;

    #[doc(hidden)]
    pub trait ToSocketAddrsPriv {
        type Iter: Iterator<Item = SocketAddr> + Send + 'static;
        type Future: Future<Output = io::Result<Self::Iter>> + Send + 'static;

        fn to_socket_addrs(&self) -> Self::Future;
    }

    #[doc(hidden)]
    #[derive(Debug)]
    #[cfg(feature = "dns")]
    pub enum MaybeReady {
        Ready(Option<SocketAddr>),
        Blocking(JoinHandle<io::Result<vec::IntoIter<SocketAddr>>>),
    }

    #[doc(hidden)]
    #[derive(Debug)]
    #[cfg(feature = "dns")]
    pub enum OneOrMore {
        One(option::IntoIter<SocketAddr>),
        More(vec::IntoIter<SocketAddr>),
    }

    #[cfg(feature = "dns")]
    impl Future for MaybeReady {
        type Output = io::Result<OneOrMore>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            use futures_core::ready;

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

    #[cfg(feature = "dns")]
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
