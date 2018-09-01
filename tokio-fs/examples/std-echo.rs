//! Echo everything received on STDIN to STDOUT.
#![deny(deprecated, warnings)]

extern crate futures;
extern crate tokio_fs;
extern crate tokio_codec;
extern crate tokio_threadpool;

use tokio_fs::{stdin, stdout, stderr};
use tokio_codec::{FramedRead, FramedWrite, LinesCodec};
use tokio_threadpool::Builder;

use futures::{Future, Stream, Sink};

use std::io;

pub fn main() {
    let pool = Builder::new()
        .pool_size(1)
        .build();

    pool.spawn({
        let input = FramedRead::new(stdin(), LinesCodec::new());

        let output = FramedWrite::new(stdout(), LinesCodec::new())
            .with(|line: String| {
                let mut out = "OUT: ".to_string();
                out.push_str(&line);
                Ok::<_, io::Error>(out)
            });

        let error = FramedWrite::new(stderr(), LinesCodec::new())
            .with(|line: String| {
                let mut out = "ERR: ".to_string();
                out.push_str(&line);
                Ok::<_, io::Error>(out)
            });

        let dst = output.fanout(error);

        input
            .forward(dst)
            .map(|_| ())
            .map_err(|e| panic!("io error = {:?}", e))
    });

    pool.shutdown_on_idle().wait().unwrap();
}
