//! Types which are documented locally in the Tokio crate, but does not actually
//! live here.
//!
//! **Note** this module is only visible on docs.rs, you cannot use it directly
//! in your own code.

#[cfg(any(feature = "net", feature = "fs", feature = "process"))]
use self::os::windows::io::{
    AsRawHandle, AsRawSocket, BorrowedHandle, BorrowedSocket, IntoRawHandle, OwnedHandle,
    OwnedSocket, RawHandle, RawSocket,
};

/// The name of a type which is not defined here.
///
/// This is typically used as an alias for another type, like so:
///
/// ```rust,ignore
/// /// See [some::other::location](https://example.com).
/// type DEFINED_ELSEWHERE = crate::doc::NotDefinedHere;
/// ```
///
/// This type is uninhabitable like the [`never` type] to ensure that no one
/// will ever accidentally use it.
///
/// [`never` type]: https://doc.rust-lang.org/std/primitive.never.html
#[derive(Debug)]
pub enum NotDefinedHere {}

/// Inherent associated functions and methods of the types this enum stands in
/// for.
///
/// `rustdoc` only type checks function bodies when it is asked to generate
/// links to definitions, which downstream projects (and `rust-analyzer`) do
/// with `-Zunstable-options --generate-link-to-definition`. Windows-only items
/// are compiled on Unix documentation builds so that docs.rs can render them,
/// so their bodies need something to resolve against. Every item below
/// diverges, as [`NotDefinedHere`] is uninhabited.
#[cfg(any(feature = "net", feature = "fs", feature = "process"))]
impl NotDefinedHere {
    /// See [`std::os::windows::io::BorrowedHandle::borrow_raw`](https://doc.rust-lang.org/std/os/windows/io/struct.BorrowedHandle.html#method.borrow_raw)
    pub const unsafe fn borrow_raw(handle: RawHandle) -> BorrowedHandle<'static> {
        match handle {}
    }

    /// See [`std::os::windows::io::BorrowedSocket::borrow_raw`](https://doc.rust-lang.org/std/os/windows/io/struct.BorrowedSocket.html#method.borrow_raw)
    pub const unsafe fn borrow_socket(sock: RawSocket) -> BorrowedSocket<'static> {
        match sock {}
    }

    /// See [`std::os::windows::io::OwnedHandle::from_raw_handle`](https://doc.rust-lang.org/std/os/windows/io/struct.OwnedHandle.html#method.from_raw_handle)
    pub const unsafe fn from_raw_handle(handle: RawHandle) -> OwnedHandle {
        match handle {}
    }

    /// See [`std::os::windows::io::OwnedSocket::from_raw_socket`](https://doc.rust-lang.org/std/os/windows/io/struct.OwnedSocket.html#method.from_raw_socket)
    pub const unsafe fn from_raw_socket(sock: RawSocket) -> OwnedSocket {
        match sock {}
    }

    /// See [`mio::windows::NamedPipe::disconnect`](https://docs.rs/mio/latest/mio/windows/struct.NamedPipe.html#method.disconnect)
    #[cfg(feature = "net")]
    pub fn disconnect(&self) -> std::io::Result<()> {
        match *self {}
    }
}

#[cfg(any(feature = "net", feature = "fs", feature = "process"))]
impl AsRawHandle for NotDefinedHere {
    fn as_raw_handle(&self) -> RawHandle {
        match *self {}
    }
}

#[cfg(any(feature = "net", feature = "fs", feature = "process"))]
impl IntoRawHandle for NotDefinedHere {
    fn into_raw_handle(self) -> RawHandle {
        match self {}
    }
}

#[cfg(feature = "net")]
impl AsRawSocket for NotDefinedHere {
    fn as_raw_socket(&self) -> RawSocket {
        match *self {}
    }
}

impl std::io::Read for NotDefinedHere {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        match *self {}
    }
}

impl std::io::Write for NotDefinedHere {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        match *self {}
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match *self {}
    }
}

impl std::io::Read for &NotDefinedHere {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        match **self {}
    }
}

impl std::io::Write for &NotDefinedHere {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        match **self {}
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match **self {}
    }
}

#[cfg(feature = "net")]
impl mio::event::Source for NotDefinedHere {
    fn register(
        &mut self,
        _registry: &mio::Registry,
        _token: mio::Token,
        _interests: mio::Interest,
    ) -> std::io::Result<()> {
        Ok(())
    }
    fn reregister(
        &mut self,
        _registry: &mio::Registry,
        _token: mio::Token,
        _interests: mio::Interest,
    ) -> std::io::Result<()> {
        Ok(())
    }
    fn deregister(&mut self, _registry: &mio::Registry) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(any(feature = "net", feature = "fs", feature = "process"))]
pub mod os;
