//! Echo everything received on STDIN to STDOUT and STDERR.

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
                // https://github.com/rust-lang/rust/pull/64856
                let s = format!("OUT: {}", line);
                output.send(s).await?;
                let s = format!("ERR: {}", line);
                error.send(s).await?;
            }
            Ok(())
        }
            .map_err(|e: LinesCodecError| panic!(e))
            .then(|_| ready(())),
    );

    pool.shutdown_on_idle().await;
    Ok(())
}
