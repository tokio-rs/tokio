# tokio-signal

An implementation of Unix signal handling for Tokio

[![Travis Build Status][travis-badge]][travis-url]
[![Appveyor Build Status][appveyor-badge]][appveyor-url]

[travis-badge]: https://travis-ci.org/tokio-rs/tokio.svg?branch=master
[travis-url]: https://travis-ci.org/tokio-rs/tokio
[appveyor-badge]: https://ci.appveyor.com/api/projects/status/s83yxhy9qeb58va7/branch/master?svg=true
[appveyor-url]: https://ci.appveyor.com/project/carllerche/tokio/branch/master

[Documentation](https://docs.rs/tokio-signal)

## Usage

First, add this to your `Cargo.toml`:

```toml
[dependencies]
tokio-signal = "0.2"
```

Next you can use this in conjunction with the `tokio` and `futures` crates:

```rust,no_run
extern crate futures;
extern crate tokio;
extern crate tokio_signal;

use futures::{Future, Stream};

fn main() {

    // Create an infinite stream of "Ctrl+C" notifications. Each item received
    // on this stream may represent multiple ctrl-c signals.
    let ctrl_c = tokio_signal::ctrl_c().flatten_stream();

    // Process each ctrl-c as it comes in
    let prog = ctrl_c.for_each(|()| {
        println!("ctrl-c received!");
        Ok(())
    });

    tokio::run(prog.map_err(|err| panic!("{}", err)));
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
for inclusion in tokio-signal by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
