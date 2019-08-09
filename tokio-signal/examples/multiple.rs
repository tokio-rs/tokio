#![warn(rust_2018_idioms)]
#![feature(async_await)]

//! A small example of how to listen for two signals at the same time

// A trick to not fail build on non-unix platforms when using unix-specific features.
#[cfg(unix)]
mod platform {
    use futures_util::stream::{self, StreamExt};
    use tokio_signal::unix::{Signal, SIGINT, SIGTERM};

    pub async fn main() {
        // Create a stream for each of the signals we'd like to handle.
        let sigint = Signal::new(SIGINT).unwrap().map(|_| SIGINT);
        let sigterm = Signal::new(SIGTERM).unwrap().map(|_| SIGTERM);

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
        let item = item.ok_or("received no signal").unwrap();
        if item == SIGINT {
            println!("received SIGINT");
        } else {
            assert_eq!(item, SIGTERM);
            println!("received SIGTERM");
        }
    }

}

#[cfg(not(unix))]
mod platform {
    pub async fn main() {}
}

#[tokio::main]
async fn main() {
    platform::main().await
}
