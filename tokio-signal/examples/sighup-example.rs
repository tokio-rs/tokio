#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use std::error::Error;

// A trick to not fail build on non-unix platforms when using unix-specific features.
#[cfg(unix)]
mod platform {
    use futures_util::stream::StreamExt;
    use std::error::Error;
    use tokio_signal::unix::{Signal, SIGHUP};

    pub async fn main() -> Result<(), Box<dyn Error>> {
        // on Unix, we can listen to whatever signal we want, in this case: SIGHUP
        let mut stream = Signal::new(SIGHUP).await?;

        println!("Waiting for SIGHUPS (Ctrl+C to quit)");
        println!(
            "  TIP: use `pkill -sighup sighup-example` from a second terminal \
             to send a SIGHUP to all processes named 'sighup-example' \
             (i.e. this binary)"
        );

        // Up until now, we haven't really DONE anything, just prepared
        // our futures, now it's time to actually await the results!
        while let Some(the_signal) = stream.next().await {
            println!(
                "*Got signal {:#x}* I should probably reload my config \
                 or something",
                the_signal
            );
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
