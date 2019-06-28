#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use futures_util::future;
use futures_util::stream::StreamExt;

/// how many signals to handle before exiting
const STOP_AFTER: u64 = 10;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // tokio_signal provides a convenience builder for Ctrl+C
    // this even works cross-platform: linux and windows!
    //
    // `fn ctrl_c()` produces a `Future` of the actual stream-initialisation
    // so first we await until the signal is ready.
    let endless_stream = tokio_signal::ctrl_c().await?;
    // don't keep going forever: convert the endless stream to a bounded one.
    let limited_stream = endless_stream.take(STOP_AFTER);

    // how many Ctrl+C have we received so far?
    let mut counter = 0;

    println!(
        "This program is now waiting for you to press Ctrl+C {0} times.
  * If running via `cargo run --example ctrl-c`, Ctrl+C also kills it, \
    due to https://github.com/rust-lang-nursery/rustup.rs/issues/806
  * If running the binary directly, the Ctrl+C is properly trapped.
    Terminate by repeating Ctrl+C {0} times, or ahead of time by \
    opening a second terminal and issuing `pkill -sigkil ctrl-c`",
        STOP_AFTER
    );

    // Stream::for_each is a powerful primitive provided by the Futures crate.
    // It turns a Stream into a Future that completes after all stream-items
    // have been completed, or the first time the closure returns an error
    let future = limited_stream
        .map(|result| result.expect("failed to get event"))
        .for_each(|()| {
            // Note how we manipulate the counter without any fancy synchronisation.
            // The borrowchecker realises there can't be any conflicts, so the closure
            // can just capture it.
            counter += 1;
            println!(
                "Ctrl+C received {} times! {} more before exit",
                counter,
                STOP_AFTER - counter
            );

            // return a result to continue handling the stream
            future::ready(())
        });

    // Up until now, we haven't really DONE anything, just prepared
    // now it's time to actually the results!
    future.await;

    println!("Stream ended, quiting the program.");
    Ok(())
}
