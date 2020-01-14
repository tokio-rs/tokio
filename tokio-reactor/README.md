# tokio-reactor

Event loop that drives Tokio I/O resources.

> **Note:** This crate is **deprecated in tokio 0.2.x** and has been moved and
> refactored into various places in the [`tokio::runtime`] and [`tokio::io`]
> modules of the [`tokio`] crate. The Reactor has also been renamed the
> "I/O Driver".

[`tokio::runtime`]: https://docs.rs/tokio/latest/tokio/runtime/index.html
[`tokio::io`]: https://docs.rs/tokio/latest/tokio/io/index.html
[`tokio`]: https://docs.rs/tokio/latest/tokio/index.html
[`io-driver` feature]: https://docs.rs/tokio/0.2.9/tokio/index.html#feature-flags

[Documentation](https://docs.rs/tokio-reactor/0.1.11/tokio_reactor)

## Overview

The reactor is the engine that drives asynchronous I/O resources (like TCP and
UDP sockets). It is backed by [`mio`] and acts as a bridge between [`mio`] and
[`futures`].

The crate provides:

* [`Reactor`] is the main type of this crate. It performs the event loop logic.

* [`Handle`] provides a reference to a reactor instance.

* [`Registration`] and [`PollEvented`] allow third parties to implement I/O
  resources that are driven by the reactor.

Application authors will not use this crate directly. Instead, they will use the
[`tokio`] crate. Library authors should only depend on `tokio-reactor` if they
are building a custom I/O resource.

[`mio`]: http://github.com/carllerche/mio
[`futures`]: http://github.com/rust-lang-nursery/futures-rs
[`Reactor`]: https://docs.rs/tokio-reactor/0.1.11/tokio_reactor/struct.Reactor.html
[`Handle`]: https://docs.rs/tokio-reactor/0.1.11/tokio_reactor/struct.Handle.html
[`Registration`]: https://docs.rs/tokio-reactor/0.1.11/tokio_reactor/struct.Registration.html
[`PollEvented`]: https://docs.rs/tokio-reactor/0.1.11/tokio_reactor/struct.PollEvented.html
[`tokio`]: ../

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tokio by you, shall be licensed as MIT, without any additional
terms or conditions.
