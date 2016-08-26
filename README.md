# tokio-core

Core I/O and event loop abstraction for asynchronous I/O in Rust built on
`futures` and `mio`.

[![Build Status](https://travis-ci.org/tokio-rs/tokio-core.svg?branch=master)](https://travis-ci.org/tokio-rs/tokio-core)
[![Build status](https://ci.appveyor.com/api/projects/status/caxmxbg8181kk9mq/branch/master?svg=true)](https://ci.appveyor.com/project/carllerche/tokio-core)

[Documentation](https://tokio-rs.github.io/tokio-core)

## Usage

First, add this to your `Cargo.toml`:

```toml
[dependencies]
tokio-core = { git = "https://github.com/tokio-rs/tokio-core" }
```

Next, add this to your crate:

```rust
extern crate tokio_core;
```

## Examples

There are a few small examples showing off how to use this library:

* [echo.rs] - a simple TCP echo server
* [socks5.rs] - an implementation of a SOCKSv5 proxy server

[echo.rs]: https://github.com/tokio-rs/tokio-core/blob/master/src/bin/echo.rs
[socks5.rs]: https://github.com/tokio-rs/tokio-socks5/blob/master/src/main.rs

## What is tokio-core?

This crate is a connection `futures`, a zero-cost implementation of futures in
Rust, and `mio` and a crate for zero-cost asynchronous I/O. The types and
structures implemented in `tokio-core` implement `Future` and `Stream` traits
as appropriate. For example connecting a TCP stream returns a `Future`
resolving to a TCP stream, and a TCP listener implements a stream of TCP
streams (accepted connections).

This crate also provides facilities such as:

* TCP streams
* TCP listeners
* UDP sockets
* Timeouts
* Data owned and local to the event loop
* An `Executor` implementation for a futures' `Task`

The intention of `tokio-core` is to provide a concrete implementation for crates
built on top of asynchronous I/O. For example you can easily turn a TCP stream
into a TLS/SSL stream with the [`tokio-tls`] crate or use the combinators to
compose working with data on sockets.

[`tokio-tls`]: https://tokio-rs.github.io/tokio-tls

Check out the [documentation] for more information, and more coming here soon!

[documentation]: https://tokio-rs.github.io/tokio-core

# License

`tokio-core` is primarily distributed under the terms of both the MIT license
and the Apache License (Version 2.0), with portions covered by various BSD-like
licenses.

See LICENSE-APACHE, and LICENSE-MIT for details.
