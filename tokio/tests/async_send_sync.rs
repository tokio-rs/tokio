#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::cell::Cell;
use std::io::Cursor;
use std::net::SocketAddr;
use std::rc::Rc;
use tokio::net::TcpStream;
use tokio::time::{Duration, Instant};

#[allow(dead_code)]
type BoxFutureSync<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + Sync>>;
#[allow(dead_code)]
type BoxFutureSend<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;
#[allow(dead_code)]
type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T>>>;

#[allow(dead_code)]
fn require_send<T: Send>(_t: &T) {}
#[allow(dead_code)]
fn require_sync<T: Sync>(_t: &T) {}

#[allow(dead_code)]
struct Invalid;

trait AmbiguousIfSend<A> {
    fn some_item(&self) {}
}
impl<T: ?Sized> AmbiguousIfSend<()> for T {}
impl<T: ?Sized + Send> AmbiguousIfSend<Invalid> for T {}

trait AmbiguousIfSync<A> {
    fn some_item(&self) {}
}
impl<T: ?Sized> AmbiguousIfSync<()> for T {}
impl<T: ?Sized + Sync> AmbiguousIfSync<Invalid> for T {}

macro_rules! into_todo {
    ($typ:ty) => {{
        let x: $typ = todo!();
        x
    }};
}
macro_rules! async_assert_fn {
    ($($f:ident)::+($($arg:ty),*): Send & Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let f = $($f)::+( $( into_todo!($arg) ),* );
            require_send(&f);
            require_sync(&f);
        };
    };
    ($($f:ident)::+($($arg:ty),*): Send & !Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let f = $($f)::+( $( into_todo!($arg) ),* );
            require_send(&f);
            AmbiguousIfSync::some_item(&f);
        };
    };
    ($($f:ident)::+($($arg:ty),*): !Send & Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let f = $($f)::+( $( into_todo!($arg) ),* );
            AmbiguousIfSend::some_item(&f);
            require_sync(&f);
        };
    };
    ($($f:ident)::+($($arg:ty),*): !Send & !Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let f = $($f)::+( $( into_todo!($arg) ),* );
            AmbiguousIfSend::some_item(&f);
            AmbiguousIfSync::some_item(&f);
        };
    };
    ($t:ty : $method:ident($($arg:ty),*): Send & Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let call_on: $t = todo!();
            let f = call_on.$method( $( into_todo!($arg) ),* );
            require_send(&f);
            require_sync(&f);
        };
    };
    ($t:ty : $method:ident($($arg:ty),*): Send & !Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let call_on: $t = todo!();
            let f = call_on.$method( $( into_todo!($arg) ),* );
            require_send(&f);
            AmbiguousIfSync::some_item(&f);
        };
    };
    ($t:ty : $method:ident($($arg:ty),*): !Send & Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let call_on: $t = todo!();
            let f = call_on.$method( $( into_todo!($arg) ),* );
            AmbiguousIfSend::some_item(&f);
            require_sync(&f);
        };
    };
    ($t:ty : $method:ident($($arg:ty),*): !Send & !Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let call_on: $t = todo!();
            let f = call_on.$method( $( into_todo!($arg) ),* );
            AmbiguousIfSend::some_item(&f);
            AmbiguousIfSync::some_item(&f);
        };
    };
}

async_assert_fn!(tokio::io::copy(&mut TcpStream, &mut TcpStream): Send & Sync);
async_assert_fn!(tokio::io::empty(): Send & Sync);
async_assert_fn!(tokio::io::repeat(u8): Send & Sync);
async_assert_fn!(tokio::io::sink(): Send & Sync);
async_assert_fn!(tokio::io::split(TcpStream): Send & Sync);
async_assert_fn!(tokio::io::stderr(): Send & Sync);
async_assert_fn!(tokio::io::stdin(): Send & Sync);
async_assert_fn!(tokio::io::stdout(): Send & Sync);
async_assert_fn!(tokio::io::Split<Cursor<Vec<u8>>>:next_segment(): Send & Sync);


async_assert_fn!(tokio::fs::canonicalize(&str): Send & Sync);
async_assert_fn!(tokio::fs::copy(&str, &str): Send & Sync);
async_assert_fn!(tokio::fs::create_dir(&str): Send & Sync);
async_assert_fn!(tokio::fs::create_dir_all(&str): Send & Sync);
async_assert_fn!(tokio::fs::hard_link(&str, &str): Send & Sync);
async_assert_fn!(tokio::fs::metadata(&str): Send & Sync);
async_assert_fn!(tokio::fs::read(&str): Send & Sync);
async_assert_fn!(tokio::fs::read_dir(&str): Send & Sync);
async_assert_fn!(tokio::fs::read_link(&str): Send & Sync);
async_assert_fn!(tokio::fs::read_to_string(&str): Send & Sync);
async_assert_fn!(tokio::fs::remove_dir(&str): Send & Sync);
async_assert_fn!(tokio::fs::remove_dir_all(&str): Send & Sync);
async_assert_fn!(tokio::fs::remove_file(&str): Send & Sync);
async_assert_fn!(tokio::fs::rename(&str, &str): Send & Sync);
async_assert_fn!(tokio::fs::set_permissions(&str, std::fs::Permissions): Send & Sync);
async_assert_fn!(tokio::fs::symlink_metadata(&str): Send & Sync);
async_assert_fn!(tokio::fs::write(&str, Vec<u8>): Send & Sync);
async_assert_fn!(tokio::fs::ReadDir:next_entry(): Send & Sync);
async_assert_fn!(tokio::fs::OpenOptions:open(&str): Send & Sync);
async_assert_fn!(tokio::fs::DirEntry:metadata(): Send & Sync);
async_assert_fn!(tokio::fs::DirEntry:file_type(): Send & Sync);

async_assert_fn!(tokio::fs::File::open(&str): Send & Sync);
async_assert_fn!(tokio::fs::File::create(&str): Send & Sync);
async_assert_fn!(tokio::fs::File:seek(std::io::SeekFrom): Send & Sync);
async_assert_fn!(tokio::fs::File:sync_all(): Send & Sync);
async_assert_fn!(tokio::fs::File:sync_data(): Send & Sync);
async_assert_fn!(tokio::fs::File:set_len(u64): Send & Sync);
async_assert_fn!(tokio::fs::File:metadata(): Send & Sync);
async_assert_fn!(tokio::fs::File:try_clone(): Send & Sync);
async_assert_fn!(tokio::fs::File:into_std(): Send & Sync);
async_assert_fn!(tokio::fs::File:set_permissions(std::fs::Permissions): Send & Sync);


async_assert_fn!(tokio::net::lookup_host(SocketAddr): Send & Sync);
async_assert_fn!(tokio::net::TcpListener::bind(SocketAddr): Send & Sync);
async_assert_fn!(tokio::net::TcpListener:accept(): Send & Sync);
async_assert_fn!(tokio::net::TcpStream::connect(SocketAddr): Send & Sync);
async_assert_fn!(tokio::net::TcpStream:peek(&mut [u8]): Send & Sync);
async_assert_fn!(tokio::net::tcp::ReadHalf<'_>:peek(&mut [u8]): Send & Sync);
async_assert_fn!(tokio::net::UdpSocket::bind(SocketAddr): Send & Sync);
async_assert_fn!(tokio::net::UdpSocket:connect(SocketAddr): Send & Sync);
async_assert_fn!(tokio::net::UdpSocket:send(&[u8]): Send & Sync);
async_assert_fn!(tokio::net::UdpSocket:recv(&mut [u8]): Send & Sync);
async_assert_fn!(tokio::net::UdpSocket:send_to(&[u8], SocketAddr): Send & Sync);
async_assert_fn!(tokio::net::UdpSocket:recv_from(&mut [u8]): Send & Sync);
async_assert_fn!(tokio::net::udp::RecvHalf:recv(&mut [u8]): Send & Sync);
async_assert_fn!(tokio::net::udp::RecvHalf:recv_from(&mut [u8]): Send & Sync);
async_assert_fn!(tokio::net::udp::SendHalf:send(&[u8]): Send & Sync);
async_assert_fn!(tokio::net::udp::SendHalf:send_to(&[u8], &SocketAddr): Send & Sync);
async_assert_fn!(tokio::net::UnixListener::bind(&str): Send & Sync);
async_assert_fn!(tokio::net::UnixListener:accept(): Send & Sync);
async_assert_fn!(tokio::net::UnixDatagram:send(&[u8]): Send & Sync);
async_assert_fn!(tokio::net::UnixDatagram:recv(&mut [u8]): Send & Sync);
async_assert_fn!(tokio::net::UnixDatagram:send_to(&[u8], &str): Send & Sync);
async_assert_fn!(tokio::net::UnixDatagram:recv_from(&mut [u8]): Send & Sync);
async_assert_fn!(tokio::net::UnixStream::connect(&str): Send & Sync);


async_assert_fn!(tokio::process::Child:wait_with_output(): Send & Sync);
async_assert_fn!(tokio::signal::ctrl_c(): Send & Sync);
#[cfg(unix)]
async_assert_fn!(tokio::signal::unix::Signal:recv(): Send & Sync);

const _: fn() = || {
    let f = tokio::stream::empty::<Rc<u8>>();
    require_send(&f);
    require_sync(&f);
};
const _: fn() = || {
    let f = tokio::stream::pending::<Rc<u8>>();
    require_send(&f);
    require_sync(&f);
};
async_assert_fn!(tokio::stream::iter(std::vec::IntoIter<u8>): Send & Sync);


async_assert_fn!(tokio::sync::Barrier:wait(): Send & Sync);
async_assert_fn!(tokio::sync::Mutex<u8>:lock(): Send & Sync);
async_assert_fn!(tokio::sync::Mutex<Cell<u8>>:lock(): Send & Sync);
async_assert_fn!(tokio::sync::Mutex<Rc<u8>>:lock(): !Send & !Sync);
async_assert_fn!(tokio::sync::Notify:notified(): Send & !Sync);
async_assert_fn!(tokio::sync::RwLock<u8>:read(): Send & Sync);
async_assert_fn!(tokio::sync::RwLock<u8>:write(): Send & Sync);
async_assert_fn!(tokio::sync::RwLock<Cell<u8>>:read(): !Send & !Sync);
async_assert_fn!(tokio::sync::RwLock<Cell<u8>>:write(): !Send & !Sync);
async_assert_fn!(tokio::sync::RwLock<Rc<u8>>:read(): !Send & !Sync);
async_assert_fn!(tokio::sync::RwLock<Rc<u8>>:write(): !Send & !Sync);
async_assert_fn!(tokio::sync::Semaphore:acquire(): Send & Sync);

async_assert_fn!(tokio::sync::broadcast::Receiver<u8>:recv(): Send & Sync);
async_assert_fn!(tokio::sync::broadcast::Receiver<Cell<u8>>:recv(): Send & Sync);
async_assert_fn!(tokio::sync::broadcast::Receiver<Rc<u8>>:recv(): !Send & !Sync);

async_assert_fn!(tokio::sync::mpsc::Receiver<u8>:recv(): Send & Sync);
async_assert_fn!(tokio::sync::mpsc::Receiver<Cell<u8>>:recv(): Send & Sync);
async_assert_fn!(tokio::sync::mpsc::Receiver<Rc<u8>>:recv(): !Send & !Sync);
async_assert_fn!(tokio::sync::mpsc::Sender<u8>:send(u8): Send & Sync);
async_assert_fn!(tokio::sync::mpsc::Sender<Cell<u8>>:send(Cell<u8>): Send & !Sync);
async_assert_fn!(tokio::sync::mpsc::Sender<Rc<u8>>:send(Rc<u8>): !Send & !Sync);

async_assert_fn!(tokio::sync::mpsc::UnboundedReceiver<u8>:recv(): Send & Sync);
async_assert_fn!(tokio::sync::mpsc::UnboundedReceiver<Cell<u8>>:recv(): Send & Sync);
async_assert_fn!(tokio::sync::mpsc::UnboundedReceiver<Rc<u8>>:recv(): !Send & !Sync);

async_assert_fn!(tokio::sync::watch::Receiver<u8>:recv(): Send & Sync);
async_assert_fn!(tokio::sync::watch::Receiver<Cell<u8>>:recv(): !Send & !Sync);
async_assert_fn!(tokio::sync::watch::Receiver<Rc<u8>>:recv(): !Send & !Sync);
async_assert_fn!(tokio::sync::watch::Sender<u8>:closed(): Send & Sync);
async_assert_fn!(tokio::sync::watch::Sender<Cell<u8>>:closed(): !Send & !Sync);
async_assert_fn!(tokio::sync::watch::Sender<Rc<u8>>:closed(): !Send & !Sync);


async_assert_fn!(tokio::task::LocalKey<u32>:scope(u32, BoxFutureSync<()>): Send & Sync);
async_assert_fn!(tokio::task::LocalKey<u32>:scope(u32, BoxFutureSend<()>): Send & !Sync);
async_assert_fn!(tokio::task::LocalKey<u32>:scope(u32, BoxFuture<()>): !Send & !Sync);
async_assert_fn!(tokio::task::LocalKey<Cell<u32>>:scope(Cell<u32>, BoxFutureSync<()>): Send & !Sync);
async_assert_fn!(tokio::task::LocalKey<Cell<u32>>:scope(Cell<u32>, BoxFutureSend<()>): Send & !Sync);
async_assert_fn!(tokio::task::LocalKey<Cell<u32>>:scope(Cell<u32>, BoxFuture<()>): !Send & !Sync);
async_assert_fn!(tokio::task::LocalKey<Rc<u32>>:scope(Rc<u32>, BoxFutureSync<()>): !Send & !Sync);
async_assert_fn!(tokio::task::LocalKey<Rc<u32>>:scope(Rc<u32>, BoxFutureSend<()>): !Send & !Sync);
async_assert_fn!(tokio::task::LocalKey<Rc<u32>>:scope(Rc<u32>, BoxFuture<()>): !Send & !Sync);
async_assert_fn!(tokio::task::LocalSet:run_until(BoxFutureSync<()>): !Send & !Sync);


async_assert_fn!(tokio::time::advance(Duration): Send & Sync);
async_assert_fn!(tokio::time::delay_for(Duration): Send & Sync);
async_assert_fn!(tokio::time::delay_until(Instant): Send & Sync);
async_assert_fn!(tokio::time::timeout(Duration, BoxFutureSync<()>): Send & Sync);
async_assert_fn!(tokio::time::timeout(Duration, BoxFutureSend<()>): Send & !Sync);
async_assert_fn!(tokio::time::timeout(Duration, BoxFuture<()>): !Send & !Sync);
async_assert_fn!(tokio::time::timeout_at(Instant, BoxFutureSync<()>): Send & Sync);
async_assert_fn!(tokio::time::timeout_at(Instant, BoxFutureSend<()>): Send & !Sync);
async_assert_fn!(tokio::time::timeout_at(Instant, BoxFuture<()>): !Send & !Sync);
async_assert_fn!(tokio::time::Interval:tick(): Send & Sync);
