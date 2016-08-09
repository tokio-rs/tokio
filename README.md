# futures-mio

Bindings to the `mio` crate implementing the `futures-io` and `futures`
abstractions.

[![Build Status](https://travis-ci.org/alexcrichton/futures-rs.svg?branch=master)](https://travis-ci.org/alexcrichton/futures-rs)
[![Build status](https://ci.appveyor.com/api/projects/status/yl5w3ittk4kggfsh?svg=true)](https://ci.appveyor.com/project/alexcrichton/futures-rs)

[Documentation](http://alexcrichton.com/futures-rs/futures_mio)

## Usage

First, add this to your `Cargo.toml`:

```toml
[dependencies]
futures-mio = { git = "https://github.com/alexcrichton/futures-rs" }
```

Next, add this to your crate:

```rust
extern crate futures_mio;
```

## Examples

There are a few small examples showing off how to use this library:

* [echo.rs] - a simple TCP echo server
* [socks5.rs] - an implementation of a SOCKSv5 proxy server

[echo]: https://github.com/alexcrichton/futures-rs/blob/master/futures-mio/src/bin/echo.rs
[socks5.rs]: https://github.com/alexcrichton/futures-rs/blob/master/futures-socks5/src/main.rs

## What is futures-mio?

This crate is a connection `futures`, a zero-cost implementation of futures in
Rust, and `mio`, a crate for zero-cost asynchronous I/O, and `futures-io`,
abstractions for I/O on top of the `futures` crate. The types and structures
implemented in `futures-mio` implement `Future` and `Stream` traits as
appropriate. For example connecting a TCP stream returns a `Future` resolving
to a TCP stream, and a TCP listener implements a stream of TCP streams
(accepted connections).

This crate also provides facilities such as:

* TCP streams
* TCP listeners
* UDP sockets
* Timeouts
* Data owned and local to the event loop
* An `Executor` implementation for a futures' `Task`

The intention of `futures-mio` is to provide a concrete implementation for
crates built on top of `futures-io`. For example you can easily turn a TCP
stream into a TLS/SSL stream with the [`futures-tls`] crate or use the
combinators to compose working with data on sockets.

[`futures-tls`]: http://alexcrichton.com/futures-rs/futures_tls

Check out the [documentation] for more information, and more coming here soon!

[documentation]: http://alexcrichton.com/futures-rs/futures_mio

# License

`futures-mio` is primarily distributed under the terms of both the MIT license
and the Apache License (Version 2.0), with portions covered by various BSD-like
licenses.

See LICENSE-APACHE, and LICENSE-MIT for details.
