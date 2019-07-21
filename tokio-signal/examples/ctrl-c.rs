#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use futures_util::stream::StreamExt;

/// how many signals to handle before exiting
const STOP_AFTER: u64 = 10;

#[tokio::main]
async fn main() {
    // tokio_signal provides a convenience builder for Ctrl+C
    // this even works cross-platform: linux and windows!
    let endless_stream = tokio_signal::CtrlC::new().expect("failed to create CtrlC");
    // don't keep going forever: convert the endless stream to a bounded one.
    let mut limited_stream = endless_stream.take(STOP_AFTER);

    // how many Ctrl+C have we received so far?
    let mut counter = 0;

    println!(
        "This program is now waiting for you to press Ctrl+C {0} times. \
         Terminate by repeating Ctrl+C {0} times, or ahead of time by opening \
         a second terminal and issuing `pkill -sigkil ctrl-c`.",
        STOP_AFTER
    );

    // Up until now, we haven't really DONE anything, just prepared
    // our futures, now it's time to actually await the results!
    while let Some(_) = limited_stream.next().await {
        // Note how we manipulate the counter without any fancy synchronisation.
        // The borrowchecker realises there can't be any conflicts, so the closure
        // can just capture it.
        counter += 1;
        println!(
            "Ctrl+C received {} times! {} more before exit",
            counter,
            STOP_AFTER - counter
        );
    }

    println!("Stream ended, quiting the program.");
}
