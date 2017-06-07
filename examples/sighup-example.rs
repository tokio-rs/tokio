extern crate futures;
extern crate tokio_core;
extern crate tokio_signal;

use futures::{Stream, Future};
use tokio_core::reactor::Core;
use tokio_signal::unix::{Signal,SIGHUP};

fn main() {
    // set up a Tokio event loop
    let mut core = Core::new().unwrap();

    // on Unix, we can listen to whatever signal we want, in this case: SIGHUP
    let stream = Signal::new(SIGHUP, &core.handle()).flatten_stream();

    println!("Waiting for SIGHUPS (Ctrl+C to quit)");
    println!("  TIP: use `pkill -sighup sighup-example` from a second terminal \
                to send a SIGHUP to all processes named 'sighup-example' \
                (i.e. this binary)");

    // for_each is a powerful primitive provided by the Futures crate
    // it turns a Stream into a Future that completes after all stream-items
    // have been completed.
    let future = stream.for_each(|the_signal| {
        println!("*Got signal {:#x}* I should probably reload my config \
                  or something", the_signal);
        Ok(())
    });

    // Up until now, we haven't really DONE anything, just prepared
    // now it's time to actually schedule, and thus execute, the stream
    // on our event loop, and loop forever
    core.run(future).unwrap();
}
