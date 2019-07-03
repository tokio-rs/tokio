#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

//! A small example of how to listen for two signals at the same time

use std::error::Error;

// A trick to not fail build on non-unix platforms when using unix-specific features.
#[cfg(unix)]
mod platform {

    use futures_util::stream::{self, StreamExt};
    use std::error::Error;
    use tokio_signal::unix::{Signal, SIGINT, SIGTERM};

    pub async fn main() -> Result<(), Box<dyn Error>> {
        // Create a stream for each of the signals we'd like to handle.
        let sigint = Signal::new(SIGINT).await?;
        let sigterm = Signal::new(SIGTERM).await?;

        // Use the `select` combinator to merge these two streams into one
        let stream = stream::select(sigint, sigterm);

        // Wait for a signal to arrive
        println!("Waiting for SIGINT or SIGTERM");
        println!(
            "  TIP: use `pkill -sigint multiple` from a second terminal \
             to send a SIGINT to all processes named 'multiple' \
             (i.e. this binary)"
        );
        let (item, _rest) = stream.into_future().await;

        // Figure out which signal we received
        let item = item.ok_or("received no signal")?;
        if item == SIGINT {
            println!("received SIGINT");
        } else {
            assert_eq!(item, SIGTERM);
            println!("received SIGTERM");
        }

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
