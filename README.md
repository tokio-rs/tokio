# Tokio

 _NOTE_: Tokio's [`master`](https://github.com/tokio-rs/tokio) branch is currently in the process of moving to [`std::future::Future`](https://doc.rust-lang.org/std/future/trait.Future.html), for `v0.1.x` based tokio releases please check out the [`v0.1.x`](https://github.com/tokio-rs/tokio/tree/v0.1.x) branch.

A runtime for writing reliable, asynchronous, and slim applications with
the Rust programming language. It is:

* **Fast**: Tokio's zero-cost abstractions give you bare-metal
  performance.

* **Reliable**: Tokio leverages Rust's ownership, type system, and
  concurrency model to reduce bugs and ensure thread safety.

* **Scalable**: Tokio has a minimal footprint, and handles backpressure
  and cancellation naturally.

[![Crates.io][crates-badge]][crates-url]
[![MIT licensed][mit-badge]][mit-url]
[![Build Status][azure-badge]][azure-url]
[![Gitter chat][gitter-badge]][gitter-url]

[crates-badge]: https://img.shields.io/crates/v/tokio.svg
[crates-url]: https://crates.io/crates/tokio
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: LICENSE
[azure-badge]: https://dev.azure.com/tokio-rs/Tokio/_apis/build/status/tokio-rs.tokio?branchName=master
[azure-url]: https://dev.azure.com/tokio-rs/Tokio/_build/latest?definitionId=1&branchName=master
[gitter-badge]: https://img.shields.io/gitter/room/tokio-rs/tokio.svg
[gitter-url]: https://gitter.im/tokio-rs/tokio

[Website](https://tokio.rs) |
[Guides](https://tokio.rs/docs/) |
[API Docs](https://docs.rs/tokio/0.1.22/tokio) |
[Chat](https://gitter.im/tokio-rs/tokio)

## Overview

Tokio is an event-driven, non-blocking I/O platform for writing
asynchronous applications with the Rust programming language. At a high
level, it provides a few major components:

* A multithreaded, work-stealing based task [scheduler].
* A [reactor] backed by the operating system's event queue (epoll, kqueue,
  IOCP, etc...).
* Asynchronous [TCP and UDP][net] sockets.

These components provide the runtime components necessary for building
an asynchronous application.

[net]: https://docs.rs/tokio/0.1.22/tokio/net/index.html
[reactor]: https://docs.rs/tokio/0.1.22/tokio/reactor/index.html
[scheduler]: https://docs.rs/tokio/0.1.22/tokio/runtime/index.html

## Example

A basic TCP echo server with Tokio:

```rust
#![feature(async_await)]

use tokio::net::TcpListener;
use tokio::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:8080".parse()?;
    let mut listener = TcpListener::bind(&addr).unwrap();

    loop {
        let (mut socket, _) = listener.accept().await?;

        tokio::spawn(async move {
            let mut buf = [0; 1024];

            // In a loop, read data from the socket and write the data back.
            loop {
                let n = match socket.read(&mut buf).await {
                    // socket closed
                    Ok(n) if n == 0 => return,
                    Ok(n) => n,
                    Err(e) => {
                        println!("failed to read from socket; err = {:?}", e);
                        return;
                    }
                };

                // Write the data back
                if let Err(e) = socket.write_all(&buf[0..n]).await {
                    println!("failed to write to socket; err = {:?}", e);
                    return;
                }
            }
        });
    }
}
```

More examples can be found [here](tokio/examples). Note that the `master` branch
is currently being updated to use `async` / `await`.  The examples are
not fully ported. Examples for stable Tokio can be found
[here](https://github.com/tokio-rs/tokio/tree/v0.1.x/tokio/examples).


## Getting Help

First, see if the answer to your question can be found in the [Guides] or the
[API documentation]. If the answer is not there, there is an active community in
the [Tokio Gitter channel][chat]. We would be happy to try to answer your
question.  Last, if that doesn't work, try opening an [issue] with the question.

[Guides]: https://tokio.rs/docs/
[API documentation]: https://docs.rs/tokio/0.1.22/tokio
[chat]: https://gitter.im/tokio-rs/tokio
[issue]: https://github.com/tokio-rs/tokio/issues/new

## Contributing

:balloon: Thanks for your help improving the project! We are so happy to have
you! We have a [contributing guide][guide] to help you get involved in the Tokio
project.

[guide]: CONTRIBUTING.md

## Project layout

The `tokio` crate, found at the root, is primarily intended for use by
application developers.  Library authors should depend on the sub crates, which
have greater guarantees of stability.

The crates included as part of Tokio are:

* [`tokio-current-thread`]: Schedule the execution of futures on the current
  thread.

* [`tokio-executor`]: Task execution related traits and utilities.

* [`tokio-fs`]: Filesystem (and standard in / out) APIs.

* [`tokio-codec`]: Utilities for encoding and decoding protocol frames.

* [`tokio-io`]: Asynchronous I/O related traits and utilities.

* [`tokio-macros`]: Macros for usage with Tokio.

* [`tokio-reactor`]: Event loop that drives I/O resources (like TCP and UDP
  sockets).

* [`tokio-tcp`]: TCP bindings for use with `tokio-io` and `tokio-reactor`.

* [`tokio-threadpool`]: Schedules the execution of futures across a pool of
  threads.

* [ `tokio-timer`]: Time related APIs.

* [`tokio-udp`]: UDP bindings for use with `tokio-io` and `tokio-reactor`.

* [`tokio-uds`]: Unix Domain Socket bindings for use with `tokio-io` and
  `tokio-reactor`.

[`tokio-codec`]: tokio-codec
[`tokio-current-thread`]: tokio-current-thread
[`tokio-executor`]: tokio-executor
[`tokio-fs`]: tokio-fs
[`tokio-io`]: tokio-io
[`tokio-macros`]: tokio-macros
[`tokio-reactor`]: tokio-reactor
[`tokio-tcp`]: tokio-tcp
[`tokio-threadpool`]: tokio-threadpool
[`tokio-timer`]: tokio-timer
[`tokio-udp`]: tokio-udp
[`tokio-uds`]: tokio-uds

## Related Projects

In addition to the crates in this repository, the Tokio project also maintains
several other libraries, including:

* [`tracing`] (formerly `tokio-trace`): A framework for application-level
  tracing and async-aware diagnostics.

* [`mio`]: A low-level, cross-platform abstraction over OS I/O APIs that powers
  `tokio`.

* [`bytes`]: Utilities for working with bytes, including efficient byte buffers.

[`tracing`]: https://github.com/tokio-rs/tracing
[`mio`]: https://github.com/tokio-rs/mio
[`bytes`]: https://github.com/tokio-rs/bytes

## Supported Rust Versions

Tokio is built against the latest stable, nightly, and beta Rust releases. The
minimum version supported is the stable release from three months before the
current stable release version. For example, if the latest stable Rust is 1.29,
the minimum version supported is 1.26. The current Tokio version is not
guaranteed to build on Rust versions earlier than the minimum supported version.

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tokio by you, shall be licensed as MIT, without any additional
terms or conditions.
