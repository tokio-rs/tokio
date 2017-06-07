extern crate futures;
extern crate tokio_core;
extern crate tokio_signal;

use futures::stream::Stream;
use tokio_core::reactor::Core;

fn main() {
    let mut core = Core::new().unwrap();
    let ctrlc = tokio_signal::ctrl_c(&core.handle());
    let stream = core.run(ctrlc).unwrap();

    println!("This program is now waiting for you to press Ctrl+C");

    core.run(stream.for_each(|()| {
        println!("Ctrl-C received!");
        Ok(())
    })).unwrap();
}
