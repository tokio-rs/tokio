#![warn(rust_2018_idioms)]
#![allow(dead_code)]

pub use tokio::runtime::current_thread::{self, Runtime as CurrentThreadRuntime};
use tokio::timer::Timeout;

#[cfg(all(unix, feature = "signal"))]
pub use tokio_net::signal::unix::{signal, SignalKind};

use bytes::{BufMut, BytesMut};
pub use futures_util::future;
use futures_util::future::FutureExt;
pub use futures_util::stream::StreamExt;
#[cfg(unix)]
use libc::{c_int, getpid, kill};
use std::future::Future;
use std::io;
use std::time::Duration;
use tokio_codec::{Decoder, Encoder};

pub fn with_timeout<F: Future>(future: F) -> impl Future<Output = F::Output> {
    Timeout::new(future, Duration::from_secs(3)).map(Result::unwrap)
}

pub fn run_with_timeout<F>(rt: &mut CurrentThreadRuntime, future: F) -> F::Output
where
    F: Future,
{
    rt.block_on(with_timeout(future))
}

#[cfg(unix)]
pub fn send_signal(signal: c_int) {
    unsafe {
        assert_eq!(kill(getpid(), signal), 0);
    }
}

pub struct ByteCodec;

impl Decoder for ByteCodec {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Vec<u8>>, io::Error> {
        let len = buf.len();
        Ok(Some(buf.split_to(len).to_vec()))
    }
}

impl Encoder for ByteCodec {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn encode(&mut self, data: Vec<u8>, buf: &mut BytesMut) -> Result<(), io::Error> {
        buf.reserve(data.len());
        buf.put(data);
        Ok(())
    }
}
