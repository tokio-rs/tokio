extern crate futures;
extern crate tokio;
extern crate tokio_signal;

// A trick to not fail build on non-unix platforms when using unix-specific features.
#[cfg(unix)]
mod platform {

    use futures::{Future, Stream};
    use tokio_signal::unix::{Signal, SIGHUP};

    pub fn main() -> Result<(), Box<::std::error::Error>> {
        // on Unix, we can listen to whatever signal we want, in this case: SIGHUP
        let stream = Signal::new(SIGHUP).flatten_stream();

        println!("Waiting for SIGHUPS (Ctrl+C to quit)");
        println!(
            "  TIP: use `pkill -sighup sighup-example` from a second terminal \
         to send a SIGHUP to all processes named 'sighup-example' \
         (i.e. this binary)"
        );

        // for_each is a powerful primitive provided by the Futures crate
        // it turns a Stream into a Future that completes after all stream-items
        // have been completed.
        let future = stream.for_each(|the_signal| {
            println!(
                "*Got signal {:#x}* I should probably reload my config \
             or something",
             the_signal
            );
            Ok(())
        });

        // Up until now, we haven't really DONE anything, just prepared
        // now it's time to actually schedule, and thus execute, the stream
        // on our event loop, and loop forever
        ::tokio::runtime::current_thread::block_on_all(future)?;
        Ok(())
    }

}

#[cfg(not(unix))]
mod platform {
    pub fn main() -> Result<(), Box<::std::error::Error>> {Ok(())}
}

fn main() -> Result<(), Box<std::error::Error>> {
    platform::main()
}
