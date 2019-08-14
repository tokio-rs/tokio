#![warn(rust_2018_idioms)]
#![feature(async_await)]

// A trick to not fail build on non-unix platforms when using unix-specific features.
#[cfg(unix)]
mod platform {
    use futures_util::stream::StreamExt;
    use tokio_signal::unix::{Signal, SignalKind};

    pub async fn main() {
        // on Unix, we can listen to whatever signal we want, in this case: SIGHUP
        let mut stream = Signal::new(SignalKind::sighup()).unwrap();

        println!("Waiting for SIGHUPS (Ctrl+C to quit)");
        println!(
            "  TIP: use `pkill -sighup sighup-example` from a second terminal \
             to send a SIGHUP to all processes named 'sighup-example' \
             (i.e. this binary)"
        );

        // Up until now, we haven't really DONE anything, just prepared
        // our futures, now it's time to actually await the results!
        while let Some(_) = stream.next().await {
            println!("*Got signal* I should probably reload my config or something");
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
