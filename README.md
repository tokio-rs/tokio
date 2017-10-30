# tokio-signal

An implementation of Unix signal handling for Tokio

[![Build Status](https://travis-ci.org/alexcrichton/tokio-signal.svg?branch=master)](https://travis-ci.org/alexcrichton/tokio-signal)

[Documentation](https://docs.rs/tokio-signal)

## Usage

First, add this to your `Cargo.toml`:

```toml
[dependencies]
tokio-signal = "0.1"
```

Next you can use this in conjunction with the `tokio-core` and `futures` crates:

```rust,no_run
extern crate futures;
extern crate tokio_core;
extern crate tokio_signal;

use tokio_core::reactor::Core;
use futures::{Future, Stream};

fn main() {
    let mut core = Core::new().unwrap();
    let handle = core.handle();

    // Create an infinite stream of "Ctrl+C" notifications. Each item received
    // on this stream may represent multiple ctrl-c signals.
    let ctrl_c = tokio_signal::ctrl_c(&handle).flatten_stream();

    // Process each ctrl-c as it comes in
    let prog = ctrl_c.for_each(|()| {
        println!("ctrl-c received!");
        Ok(())
    });

    core.run(prog).unwrap();
}
```

# License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Serde by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
