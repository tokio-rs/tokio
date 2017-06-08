extern crate futures;
extern crate tokio_core;
extern crate tokio_signal;

use futures::{Stream, Future};
use tokio_core::reactor::Core;

fn main() {
    // set up a Tokio event loop
    let mut core = Core::new().unwrap();

    // tokio_signal provides a convenience builder for Ctrl+C
    // this even works cross-platform: linux and windows!
    //
    // `fn ctrl_c()` produces a `Future` of the actual stream-initialisation
    // the `flatten_stream()` convenience method lazily defers that
    // initialisation, allowing us to use it 'as if' it is already the
    // stream we want, reducing boilerplate Future-handling.
    let stream = tokio_signal::ctrl_c(&core.handle()).flatten_stream();

    println!("This program is now waiting for you to press Ctrl+C");

    // for_each is a powerful primitive provided by the Futures crate
    // it turns a Stream into a Future that completes after all stream-items
    // have been completed.
    let future = stream.for_each(|()| {
        println!("Ctrl+C received!");
        Ok(())
    });

    // Up until now, we haven't really DONE anything, just prepared
    // now it's time to actually schedule, and thus execute, the stream
    // on our event loop
    core.run(future).unwrap();

    println!("this won't be printed, because the received Ctrl+C will also kill the program");
    unreachable!();
}
