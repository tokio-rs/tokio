use tokio_executor::blocking;

use futures_util::future;
use std::future::Future;
use std::io;
use std::net::{SocketAddr, IpAddr};

/// Convert or resolve without blocking to one or more `SocketAddr` values.
pub trait ToSocketAddrs: sealed::ToSocketAddrsPriv {
}

type BoxFuture<T> = Box<dyn Future<Output = io::Result<T>> + Unpin + Send>;

// ===== impl SocketAddr =====

impl ToSocketAddrs for SocketAddr {}

impl sealed::ToSocketAddrsPriv for SocketAddr {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = BoxFuture<Self::Iter>;

    fn to_socket_addrs(&self) -> Self::Future {
        let iter = Some(self.clone()).into_iter();
        Box::new(future::ready(Ok(iter)))
    }
}

// ===== impl str =====

impl ToSocketAddrs for str {}

impl sealed::ToSocketAddrsPriv for str {
    type Iter = std::vec::IntoIter<SocketAddr>;
    type Future = BoxFuture<Self::Iter>;

    fn to_socket_addrs(&self) -> Self::Future {
        // First check if the input parses as a socket address
        let res: Result<SocketAddr, _> = self.parse();

        if let Ok(addr) = res {
            let iter = vec![addr].into_iter();
            return Box::new(future::ready(Ok(iter)));
        }

        // Run DNS lookup on the blocking pool
        let s = self.to_owned();

        Box::new(blocking::run(move || {
            std::net::ToSocketAddrs::to_socket_addrs(&s)
        }))
    }
}

// ===== impl (IpAddr, u16) =====

impl ToSocketAddrs for (IpAddr, u16) {}

impl sealed::ToSocketAddrsPriv for (IpAddr, u16) {
    type Iter = std::option::IntoIter<SocketAddr>;
    type Future = BoxFuture<Self::Iter>;

    fn to_socket_addrs(&self) -> Self::Future {
        let iter = Some(SocketAddr::from(*self)).into_iter();
        Box::new(future::ready(Ok(iter)))
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
    use std::future::Future;
    use std::io;
    use std::net::SocketAddr;

    pub trait ToSocketAddrsPriv {
        type Iter: Iterator<Item = SocketAddr>;
        type Future: Future<Output = io::Result<Self::Iter>>;

        fn to_socket_addrs(&self) -> Self::Future;
    }
}
