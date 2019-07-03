#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use std::error::Error;

// A trick to not fail build on non-unix platforms when using unix-specific features.
#[cfg(unix)]
mod platform {

    use futures_util::future;
    use futures_util::stream::StreamExt;
    use std::error::Error;
    use tokio_signal::unix::{Signal, SIGHUP};

    pub async fn main() -> Result<(), Box<dyn Error>> {
        // on Unix, we can listen to whatever signal we want, in this case: SIGHUP
        let stream = Signal::new(SIGHUP).await?;

        println!("Waiting for SIGHUPS (Ctrl+C to quit)");
        println!(
            "  TIP: use `pkill -sighup sighup-example` from a second terminal \
             to send a SIGHUP to all processes named 'sighup-example' \
             (i.e. this binary)"
        );

        // for_each is a powerful primitive provided by the Futures crate
        // it turns a Stream into a Future that completes after all stream-items
        // have been completed.
        let future = stream.for_each(|the_signal| {
            println!(
                "*Got signal {:#x}* I should probably reload my config \
                 or something",
                the_signal
            );

            future::ready(())
        });

        // Up until now, we haven't really DONE anything, just prepared
        // now it's time to actually the results!
        future.await;

        Ok(())
    }
}

#[cfg(not(unix))]
mod platform {
    use std::error::Error;
    pub async fn main() -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    platform::main().await
}
