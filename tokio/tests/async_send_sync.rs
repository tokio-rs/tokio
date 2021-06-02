#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]
#![allow(clippy::type_complexity, clippy::diverging_sub_expression)]

use std::cell::Cell;
use std::future::Future;
use std::io::{Cursor, SeekFrom};
use std::net::SocketAddr;
use std::pin::Pin;
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
type BoxAsyncRead = std::pin::Pin<Box<dyn tokio::io::AsyncBufRead>>;
#[allow(dead_code)]
type BoxAsyncSeek = std::pin::Pin<Box<dyn tokio::io::AsyncSeek>>;
#[allow(dead_code)]
type BoxAsyncWrite = std::pin::Pin<Box<dyn tokio::io::AsyncWrite>>;

#[allow(dead_code)]
fn require_send<T: Send>(_t: &T) {}
#[allow(dead_code)]
fn require_sync<T: Sync>(_t: &T) {}
#[allow(dead_code)]
fn require_unpin<T: Unpin>(_t: &T) {}

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

trait AmbiguousIfUnpin<A> {
    fn some_item(&self) {}
}
impl<T: ?Sized> AmbiguousIfUnpin<()> for T {}
impl<T: ?Sized + Unpin> AmbiguousIfUnpin<Invalid> for T {}

macro_rules! into_todo {
    ($typ:ty) => {{
        let x: $typ = todo!();
        x
    }};
}
macro_rules! assert_value {
    ($type:ty: Send & Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let f: $type = todo!();
            require_send(&f);
            require_sync(&f);
        };
    };
    ($type:ty: !Send & Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let f: $type = todo!();
            AmbiguousIfSend::some_item(&f);
            require_sync(&f);
        };
    };
    ($type:ty: Send & !Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let f: $type = todo!();
            require_send(&f);
            AmbiguousIfSync::some_item(&f);
        };
    };
    ($type:ty: !Send & !Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let f: $type = todo!();
            AmbiguousIfSend::some_item(&f);
            AmbiguousIfSync::some_item(&f);
        };
    };
    ($type:ty: Unpin) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let f: $type = todo!();
            require_unpin(&f);
        };
    };
}
macro_rules! async_assert_fn {
    ($($f:ident $(< $($generic:ty),* > )? )::+($($arg:ty),*): Send & Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let f = $($f $(::<$($generic),*>)? )::+( $( into_todo!($arg) ),* );
            require_send(&f);
            require_sync(&f);
        };
    };
    ($($f:ident $(< $($generic:ty),* > )? )::+($($arg:ty),*): Send & !Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let f = $($f $(::<$($generic),*>)? )::+( $( into_todo!($arg) ),* );
            require_send(&f);
            AmbiguousIfSync::some_item(&f);
        };
    };
    ($($f:ident $(< $($generic:ty),* > )? )::+($($arg:ty),*): !Send & Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let f = $($f $(::<$($generic),*>)? )::+( $( into_todo!($arg) ),* );
            AmbiguousIfSend::some_item(&f);
            require_sync(&f);
        };
    };
    ($($f:ident $(< $($generic:ty),* > )? )::+($($arg:ty),*): !Send & !Sync) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let f = $($f $(::<$($generic),*>)? )::+( $( into_todo!($arg) ),* );
            AmbiguousIfSend::some_item(&f);
            AmbiguousIfSync::some_item(&f);
        };
    };
    ($($f:ident $(< $($generic:ty),* > )? )::+($($arg:ty),*): !Unpin) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let f = $($f $(::<$($generic),*>)? )::+( $( into_todo!($arg) ),* );
            AmbiguousIfUnpin::some_item(&f);
        };
    };
    ($($f:ident $(< $($generic:ty),* > )? )::+($($arg:ty),*): Unpin) => {
        #[allow(unreachable_code)]
        #[allow(unused_variables)]
        const _: fn() = || {
            let f = $($f $(::<$($generic),*>)? )::+( $( into_todo!($arg) ),* );
            require_unpin(&f);
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
async_assert_fn!(tokio::io::Split<Cursor<Vec<u8>>>::next_segment(_): Send & Sync);

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
async_assert_fn!(tokio::fs::ReadDir::next_entry(_): Send & Sync);
async_assert_fn!(tokio::fs::OpenOptions::open(_, &str): Send & Sync);
async_assert_fn!(tokio::fs::DirEntry::metadata(_): Send & Sync);
async_assert_fn!(tokio::fs::DirEntry::file_type(_): Send & Sync);

async_assert_fn!(tokio::fs::File::open(&str): Send & Sync);
async_assert_fn!(tokio::fs::File::create(&str): Send & Sync);
async_assert_fn!(tokio::fs::File::sync_all(_): Send & Sync);
async_assert_fn!(tokio::fs::File::sync_data(_): Send & Sync);
async_assert_fn!(tokio::fs::File::set_len(_, u64): Send & Sync);
async_assert_fn!(tokio::fs::File::metadata(_): Send & Sync);
async_assert_fn!(tokio::fs::File::try_clone(_): Send & Sync);
async_assert_fn!(tokio::fs::File::into_std(_): Send & Sync);
async_assert_fn!(tokio::fs::File::set_permissions(_, std::fs::Permissions): Send & Sync);

async_assert_fn!(tokio::net::lookup_host(SocketAddr): Send & Sync);
async_assert_fn!(tokio::net::TcpListener::bind(SocketAddr): Send & Sync);
async_assert_fn!(tokio::net::TcpListener::accept(_): Send & Sync);
async_assert_fn!(tokio::net::TcpStream::connect(SocketAddr): Send & Sync);
async_assert_fn!(tokio::net::TcpStream::peek(_, &mut [u8]): Send & Sync);
async_assert_fn!(tokio::net::tcp::ReadHalf::peek(_, &mut [u8]): Send & Sync);
async_assert_fn!(tokio::net::UdpSocket::bind(SocketAddr): Send & Sync);
async_assert_fn!(tokio::net::UdpSocket::connect(_, SocketAddr): Send & Sync);
async_assert_fn!(tokio::net::UdpSocket::send(_, &[u8]): Send & Sync);
async_assert_fn!(tokio::net::UdpSocket::recv(_, &mut [u8]): Send & Sync);
async_assert_fn!(tokio::net::UdpSocket::send_to(_, &[u8], SocketAddr): Send & Sync);
async_assert_fn!(tokio::net::UdpSocket::recv_from(_, &mut [u8]): Send & Sync);

#[cfg(unix)]
mod unix_datagram {
    use super::*;
    async_assert_fn!(tokio::net::UnixListener::bind(&str): Send & Sync);
    async_assert_fn!(tokio::net::UnixListener::accept(_): Send & Sync);
    async_assert_fn!(tokio::net::UnixDatagram::send(_, &[u8]): Send & Sync);
    async_assert_fn!(tokio::net::UnixDatagram::recv(_, &mut [u8]): Send & Sync);
    async_assert_fn!(tokio::net::UnixDatagram::send_to(_, &[u8], &str): Send & Sync);
    async_assert_fn!(tokio::net::UnixDatagram::recv_from(_, &mut [u8]): Send & Sync);
    async_assert_fn!(tokio::net::UnixStream::connect(&str): Send & Sync);
}

async_assert_fn!(tokio::process::Child::wait_with_output(_): Send & Sync);
async_assert_fn!(tokio::signal::ctrl_c(): Send & Sync);
#[cfg(unix)]
async_assert_fn!(tokio::signal::unix::Signal::recv(_): Send & Sync);

async_assert_fn!(tokio::sync::Barrier::wait(_): Send & Sync);
async_assert_fn!(tokio::sync::Mutex<u8>::lock(_): Send & Sync);
async_assert_fn!(tokio::sync::Mutex<Cell<u8>>::lock(_): Send & Sync);
async_assert_fn!(tokio::sync::Mutex<Rc<u8>>::lock(_): !Send & !Sync);
async_assert_fn!(tokio::sync::Mutex<u8>::lock_owned(_): Send & Sync);
async_assert_fn!(tokio::sync::Mutex<Cell<u8>>::lock_owned(_): Send & Sync);
async_assert_fn!(tokio::sync::Mutex<Rc<u8>>::lock_owned(_): !Send & !Sync);
async_assert_fn!(tokio::sync::Notify::notified(_): Send & Sync);
async_assert_fn!(tokio::sync::RwLock<u8>::read(_): Send & Sync);
async_assert_fn!(tokio::sync::RwLock<u8>::write(_): Send & Sync);
async_assert_fn!(tokio::sync::RwLock<Cell<u8>>::read(_): !Send & !Sync);
async_assert_fn!(tokio::sync::RwLock<Cell<u8>>::write(_): !Send & !Sync);
async_assert_fn!(tokio::sync::RwLock<Rc<u8>>::read(_): !Send & !Sync);
async_assert_fn!(tokio::sync::RwLock<Rc<u8>>::write(_): !Send & !Sync);
async_assert_fn!(tokio::sync::Semaphore::acquire(_): Send & Sync);

async_assert_fn!(tokio::sync::broadcast::Receiver<u8>::recv(_): Send & Sync);
async_assert_fn!(tokio::sync::broadcast::Receiver<Cell<u8>>::recv(_): Send & Sync);
async_assert_fn!(tokio::sync::broadcast::Receiver<Rc<u8>>::recv(_): !Send & !Sync);

async_assert_fn!(tokio::sync::mpsc::Receiver<u8>::recv(_): Send & Sync);
async_assert_fn!(tokio::sync::mpsc::Receiver<Cell<u8>>::recv(_): Send & Sync);
async_assert_fn!(tokio::sync::mpsc::Receiver<Rc<u8>>::recv(_): !Send & !Sync);
async_assert_fn!(tokio::sync::mpsc::Sender<u8>::send(_, u8): Send & Sync);
async_assert_fn!(tokio::sync::mpsc::Sender<Cell<u8>>::send(_, Cell<u8>): Send & !Sync);
async_assert_fn!(tokio::sync::mpsc::Sender<Rc<u8>>::send(_, Rc<u8>): !Send & !Sync);

async_assert_fn!(tokio::sync::mpsc::UnboundedReceiver<u8>::recv(_): Send & Sync);
async_assert_fn!(tokio::sync::mpsc::UnboundedReceiver<Cell<u8>>::recv(_): Send & Sync);
async_assert_fn!(tokio::sync::mpsc::UnboundedReceiver<Rc<u8>>::recv(_): !Send & !Sync);

async_assert_fn!(tokio::sync::watch::Receiver<u8>::changed(_): Send & Sync);
async_assert_fn!(tokio::sync::watch::Sender<u8>::closed(_): Send & Sync);
async_assert_fn!(tokio::sync::watch::Sender<Cell<u8>>::closed(_): !Send & !Sync);
async_assert_fn!(tokio::sync::watch::Sender<Rc<u8>>::closed(_): !Send & !Sync);

async_assert_fn!(tokio::sync::OnceCell<u8>::get_or_init(
    _, fn() -> Pin<Box<dyn Future<Output = u8> + Send + Sync>>): Send & Sync);
async_assert_fn!(tokio::sync::OnceCell<u8>::get_or_init(
    _, fn() -> Pin<Box<dyn Future<Output = u8> + Send>>): Send & !Sync);
async_assert_fn!(tokio::sync::OnceCell<u8>::get_or_init(
    _, fn() -> Pin<Box<dyn Future<Output = u8>>>): !Send & !Sync);
async_assert_fn!(tokio::sync::OnceCell<Cell<u8>>::get_or_init(
    _, fn() -> Pin<Box<dyn Future<Output = Cell<u8>> + Send + Sync>>): !Send & !Sync);
async_assert_fn!(tokio::sync::OnceCell<Cell<u8>>::get_or_init(
    _, fn() -> Pin<Box<dyn Future<Output = Cell<u8>> + Send>>): !Send & !Sync);
async_assert_fn!(tokio::sync::OnceCell<Cell<u8>>::get_or_init(
    _, fn() -> Pin<Box<dyn Future<Output = Cell<u8>>>>): !Send & !Sync);
async_assert_fn!(tokio::sync::OnceCell<Rc<u8>>::get_or_init(
    _, fn() -> Pin<Box<dyn Future<Output = Rc<u8>> + Send + Sync>>): !Send & !Sync);
async_assert_fn!(tokio::sync::OnceCell<Rc<u8>>::get_or_init(
    _, fn() -> Pin<Box<dyn Future<Output = Rc<u8>> + Send>>): !Send & !Sync);
async_assert_fn!(tokio::sync::OnceCell<Rc<u8>>::get_or_init(
    _, fn() -> Pin<Box<dyn Future<Output = Rc<u8>>>>): !Send & !Sync);
assert_value!(tokio::sync::OnceCell<u8>: Send & Sync);
assert_value!(tokio::sync::OnceCell<Cell<u8>>: Send & !Sync);
assert_value!(tokio::sync::OnceCell<Rc<u8>>: !Send & !Sync);

async_assert_fn!(tokio::task::LocalKey<u32>::scope(_, u32, BoxFutureSync<()>): Send & Sync);
async_assert_fn!(tokio::task::LocalKey<u32>::scope(_, u32, BoxFutureSend<()>): Send & !Sync);
async_assert_fn!(tokio::task::LocalKey<u32>::scope(_, u32, BoxFuture<()>): !Send & !Sync);
async_assert_fn!(tokio::task::LocalKey<Cell<u32>>::scope(_, Cell<u32>, BoxFutureSync<()>): Send & !Sync);
async_assert_fn!(tokio::task::LocalKey<Cell<u32>>::scope(_, Cell<u32>, BoxFutureSend<()>): Send & !Sync);
async_assert_fn!(tokio::task::LocalKey<Cell<u32>>::scope(_, Cell<u32>, BoxFuture<()>): !Send & !Sync);
async_assert_fn!(tokio::task::LocalKey<Rc<u32>>::scope(_, Rc<u32>, BoxFutureSync<()>): !Send & !Sync);
async_assert_fn!(tokio::task::LocalKey<Rc<u32>>::scope(_, Rc<u32>, BoxFutureSend<()>): !Send & !Sync);
async_assert_fn!(tokio::task::LocalKey<Rc<u32>>::scope(_, Rc<u32>, BoxFuture<()>): !Send & !Sync);
async_assert_fn!(tokio::task::LocalSet::run_until(_, BoxFutureSync<()>): !Send & !Sync);
assert_value!(tokio::task::LocalSet: !Send & !Sync);

async_assert_fn!(tokio::time::advance(Duration): Send & Sync);
async_assert_fn!(tokio::time::sleep(Duration): Send & Sync);
async_assert_fn!(tokio::time::sleep_until(Instant): Send & Sync);
async_assert_fn!(tokio::time::timeout(Duration, BoxFutureSync<()>): Send & Sync);
async_assert_fn!(tokio::time::timeout(Duration, BoxFutureSend<()>): Send & !Sync);
async_assert_fn!(tokio::time::timeout(Duration, BoxFuture<()>): !Send & !Sync);
async_assert_fn!(tokio::time::timeout_at(Instant, BoxFutureSync<()>): Send & Sync);
async_assert_fn!(tokio::time::timeout_at(Instant, BoxFutureSend<()>): Send & !Sync);
async_assert_fn!(tokio::time::timeout_at(Instant, BoxFuture<()>): !Send & !Sync);
async_assert_fn!(tokio::time::Interval::tick(_): Send & Sync);

assert_value!(tokio::time::Interval: Unpin);
async_assert_fn!(tokio::time::sleep(Duration): !Unpin);
async_assert_fn!(tokio::time::sleep_until(Instant): !Unpin);
async_assert_fn!(tokio::time::timeout(Duration, BoxFuture<()>): !Unpin);
async_assert_fn!(tokio::time::timeout_at(Instant, BoxFuture<()>): !Unpin);
async_assert_fn!(tokio::time::Interval::tick(_): !Unpin);
async_assert_fn!(tokio::io::AsyncBufReadExt::read_until(&mut BoxAsyncRead, u8, &mut Vec<u8>): !Unpin);
async_assert_fn!(tokio::io::AsyncBufReadExt::read_line(&mut BoxAsyncRead, &mut String): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read(&mut BoxAsyncRead, &mut [u8]): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_exact(&mut BoxAsyncRead, &mut [u8]): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_u8(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_i8(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_u16(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_i16(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_u32(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_i32(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_u64(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_i64(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_u128(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_i128(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_u16_le(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_i16_le(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_u32_le(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_i32_le(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_u64_le(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_i64_le(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_u128_le(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_i128_le(&mut BoxAsyncRead): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_to_end(&mut BoxAsyncRead, &mut Vec<u8>): !Unpin);
async_assert_fn!(tokio::io::AsyncReadExt::read_to_string(&mut BoxAsyncRead, &mut String): !Unpin);
async_assert_fn!(tokio::io::AsyncSeekExt::seek(&mut BoxAsyncSeek, SeekFrom): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write(&mut BoxAsyncWrite, &[u8]): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_all(&mut BoxAsyncWrite, &[u8]): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_u8(&mut BoxAsyncWrite, u8): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_i8(&mut BoxAsyncWrite, i8): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_u16(&mut BoxAsyncWrite, u16): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_i16(&mut BoxAsyncWrite, i16): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_u32(&mut BoxAsyncWrite, u32): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_i32(&mut BoxAsyncWrite, i32): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_u64(&mut BoxAsyncWrite, u64): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_i64(&mut BoxAsyncWrite, i64): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_u128(&mut BoxAsyncWrite, u128): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_i128(&mut BoxAsyncWrite, i128): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_u16_le(&mut BoxAsyncWrite, u16): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_i16_le(&mut BoxAsyncWrite, i16): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_u32_le(&mut BoxAsyncWrite, u32): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_i32_le(&mut BoxAsyncWrite, i32): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_u64_le(&mut BoxAsyncWrite, u64): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_i64_le(&mut BoxAsyncWrite, i64): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_u128_le(&mut BoxAsyncWrite, u128): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::write_i128_le(&mut BoxAsyncWrite, i128): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::flush(&mut BoxAsyncWrite): !Unpin);
async_assert_fn!(tokio::io::AsyncWriteExt::shutdown(&mut BoxAsyncWrite): !Unpin);
