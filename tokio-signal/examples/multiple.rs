//! A small example of how to listen for two signals at the same time

extern crate futures;
extern crate tokio;
extern crate tokio_signal;

// A trick to not fail build on non-unix platforms when using unix-specific features.
#[cfg(unix)]
mod platform {

    use futures::{Future, Stream};
    use tokio_signal::unix::{Signal, SIGINT, SIGTERM};

    pub fn main() {
        // Create a stream for each of the signals we'd like to handle.
        let sigint = Signal::new(SIGINT).flatten_stream();
        let sigterm = Signal::new(SIGTERM).flatten_stream();

        // Use the `select` combinator to merge these two streams into one
        let stream = sigint.select(sigterm);

        // Wait for a signal to arrive
        println!("Waiting for SIGINT or SIGTERM");
        println!(
            "  TIP: use `pkill -sigint multiple` from a second terminal \
         to send a SIGINT to all processes named 'multiple' \
         (i.e. this binary)"
        );
        let (item, _rest) = ::tokio::runtime::current_thread::block_on_all(stream.into_future())
            .ok()
            .unwrap();

        // Figure out which signal we received
        let item = item.unwrap();
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
    pub fn main() {}
}

fn main() {
    platform::main()
}
