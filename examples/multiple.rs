//! A small example of how to listen for two signals at the same time

extern crate futures;
extern crate tokio_core;
extern crate tokio_signal;

use futures::{Future, Stream};
use tokio_core::reactor::Core;
use tokio_signal::unix::{Signal, SIGINT, SIGTERM};

fn main() {
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let handle = handle.new_tokio_handle();

    // Create a stream for each of the signals we'd like to handle.
    let sigint = Signal::new(SIGINT, &handle).flatten_stream();
    let sigterm = Signal::new(SIGTERM, &handle).flatten_stream();

    // Use the `select` combinator to merge these two streams into one
    let stream = sigint.select(sigterm);

    // Wait for a signal to arrive
    println!("Waiting for SIGINT or SIGTERM");
    println!(
        "  TIP: use `pkill -sigint multiple` from a second terminal \
         to send a SIGINT to all processes named 'multiple' \
         (i.e. this binary)"
    );
    let (item, _rest) = core.run(stream.into_future()).ok().unwrap();

    // Figure out which signal we received
    let item = item.unwrap();
    if item == SIGINT {
        println!("received SIGINT");
    } else {
        assert_eq!(item, SIGTERM);
        println!("received SIGTERM");
    }
}
