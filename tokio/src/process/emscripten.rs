//! Throwing process stub for `wasm32-unknown-emscripten`.
//!
//! emscripten has no `fork`/`exec`, so there is nothing to spawn or reap: the
//! orphan reaper / signal driver the unix backend relies on is compiled out
//! (see `cfg_process_driver!`). We still provide the `imp` surface that
//! `process::mod` is generic over so dependents that merely name these types
//! keep compiling. Spawning fails before it reaches here (`std`'s own
//! `Command::spawn` returns `Unsupported` on emscripten), and the `Child` /
//! `ChildStdio` types are uninhabited — every method is unreachable.

use crate::io::{AsyncRead, AsyncWrite, ReadBuf};
use crate::process::kill::Kill;
use crate::process::SpawnedChild;

use std::fmt;
use std::future::Future;
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, IntoRawFd, OwnedFd, RawFd};
use std::pin::Pin;
use std::process::{Child as StdChild, ExitStatus, Stdio};
use std::task::{Context, Poll};

fn unsupported<T>() -> io::Result<T> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "spawning processes is not supported on wasm32-unknown-emscripten",
    ))
}

pub(crate) enum Child {}

impl Child {
    pub(crate) fn id(&self) -> u32 {
        match *self {}
    }

    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match *self {}
    }
}

impl fmt::Debug for Child {
    fn fmt(&self, _fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {}
    }
}

impl Kill for Child {
    fn kill(&mut self) -> io::Result<()> {
        match *self {}
    }
}

impl Future for Child {
    type Output = io::Result<ExitStatus>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        match *self.get_mut() {}
    }
}

pub(crate) fn build_child(_child: StdChild) -> io::Result<SpawnedChild> {
    unsupported()
}

pub(crate) enum ChildStdio {}

impl ChildStdio {
    pub(crate) fn into_owned_fd(self) -> io::Result<OwnedFd> {
        match self {}
    }
}

impl fmt::Debug for ChildStdio {
    fn fmt(&self, _fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {}
    }
}

impl AsRawFd for ChildStdio {
    fn as_raw_fd(&self) -> RawFd {
        match *self {}
    }
}

impl AsFd for ChildStdio {
    fn as_fd(&self) -> BorrowedFd<'_> {
        match *self {}
    }
}

impl AsyncWrite for ChildStdio {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match *self.get_mut() {}
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match *self.get_mut() {}
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match *self.get_mut() {}
    }
}

impl AsyncRead for ChildStdio {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match *self.get_mut() {}
    }
}

pub(crate) fn convert_to_stdio(io: ChildStdio) -> io::Result<Stdio> {
    match io {}
}

pub(crate) fn stdio<T: IntoRawFd>(io: T) -> io::Result<ChildStdio> {
    let _ = io.into_raw_fd();
    unsupported()
}
