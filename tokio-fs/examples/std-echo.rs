//! Echo everything received on STDIN to STDOUT and STDERR.
#![feature(async_await)]

use futures_util::{FutureExt, SinkExt, StreamExt, TryFutureExt};

use tokio::codec::{FramedRead, FramedWrite, LinesCodec, LinesCodecError};
use tokio::future::ready;
use tokio_executor::threadpool::Builder;
use tokio_fs::{stderr, stdin, stdout};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = Builder::new().pool_size(1).build();

    pool.spawn(
        async {
            let mut input = FramedRead::new(stdin(), LinesCodec::new());
            let mut output = FramedWrite::new(stdout(), LinesCodec::new());
            let mut error = FramedWrite::new(stderr(), LinesCodec::new());

            while let Some(line) = input.next().await {
                let line = line?;
                output.send(format!("OUT: {}", line)).await?;
                error.send(format!("ERR: {}", line)).await?;
            }
            Ok(())
        }
            .map_err(|e: LinesCodecError| panic!(e))
            .then(|_| ready(())),
    );

    pool.shutdown_on_idle().await;
    Ok(())
}
