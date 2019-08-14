//! Echo everything received on STDIN to STDOUT.
#![feature(async_await)]

use tokio_codec::{FramedRead, FramedWrite, LinesCodec};
use tokio_fs::{stderr, stdin, stdout};
use tokio_threadpool::Builder;

use futures_util::sink::SinkExt;

use std::io;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = Builder::new().pool_size(1).build();

    pool.spawn({
        let input = FramedRead::new(stdin(), LinesCodec::new());

        let output = FramedWrite::new(stdout(), LinesCodec::new()).with(|line: String| {
            let mut out = "OUT: ".to_string();
            out.push_str(&line);
            Ok::<_, io::Error>(out)
        });

        let error = FramedWrite::new(stderr(), LinesCodec::new()).with(|line: String| {
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

    pool.shutdown_on_idle()
        .wait()
        .map_err(|_| "failed to shutdown the thread pool")?;
    Ok(())
}
