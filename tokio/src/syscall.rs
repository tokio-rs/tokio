use crate::io::Registration;
use crate::net::tcp;
use std::{io, net, vec};

trait Syscall {
    /// Bind Evented to the reactor, panics if no reactor is currently running.
    fn register_io<T>(&self, io: T) -> io::Result<Registration>
    where
        T: mio::Evented;

    fn tcp_listener_bind_addr(&self, addr: net::SocketAddr) -> io::Result<tcp::ListenerInner>;

    fn tcp_stream_bind_addr(&self, addr: net::SocketAddr) -> io::Result<tcp::StreamInner>;

    fn resolve_str_addr(&self, addr: &str) -> io::Result<vec::IntoIter<net::SocketAddr>>;

    fn resolve_tuple_addr(&self, addr: &(&str, u16)) -> io::Result<vec::IntoIter<net::SocketAddr>>;
}
