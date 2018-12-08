# Tokio

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
[![Travis Build Status][travis-badge]][travis-url]
[![Appveyor Build Status][appveyor-badge]][appveyor-url]
[![Gitter chat][gitter-badge]][gitter-url]

[crates-badge]: https://img.shields.io/crates/v/tokio.svg
[crates-url]: https://crates.io/crates/tokio
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: LICENSE-MIT
[travis-badge]: https://travis-ci.org/tokio-rs/tokio.svg?branch=master
[travis-url]: https://travis-ci.org/tokio-rs/tokio
[appveyor-badge]: https://ci.appveyor.com/api/projects/status/s83yxhy9qeb58va7/branch/master?svg=true
[appveyor-url]: https://ci.appveyor.com/project/carllerche/tokio/branch/master
[gitter-badge]: https://img.shields.io/gitter/room/tokio-rs/tokio.svg
[gitter-url]: https://gitter.im/tokio-rs/tokio

[Website](https://tokio.rs) |
[Guides](https://tokio.rs/docs/getting-started/hello-world/) |
[API Docs](https://docs.rs/tokio) |
[Chat](https://gitter.im/tokio-rs/tokio)

The API docs for the master branch are published [here][master-dox].

[master-dox]: https://tokio-rs.github.io/tokio/tokio/

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

[net]: https://docs.rs/tokio/0.1/tokio/net/index.html
[reactor]: https://docs.rs/tokio/0.1/tokio/reactor/index.html
[scheduler]: https://tokio-rs.github.io/tokio/tokio/runtime/index.html

## Example

A basic TCP echo server with Tokio:

```rust
extern crate tokio;

use tokio::prelude::*;
use tokio::io::copy;
use tokio::net::TcpListener;

fn main() {
    // Bind the server's socket.
    let addr = "127.0.0.1:12345".parse().unwrap();
    let listener = TcpListener::bind(&addr)
        .expect("unable to bind TCP listener");

    // Pull out a stream of sockets for incoming connections
    let server = listener.incoming()
        .map_err(|e| eprintln!("accept failed = {:?}", e))
        .for_each(|sock| {
            // Split up the reading and writing parts of the
            // socket.
            let (reader, writer) = sock.split();

            // A future that echos the data and returns how
            // many bytes were copied...
            let bytes_copied = copy(reader, writer);

            // ... after which we'll print what happened.
            let handle_conn = bytes_copied.map(|amt| {
                println!("wrote {:?} bytes", amt)
            }).map_err(|err| {
                eprintln!("IO error {:?}", err)
            });

            // Spawn the future as a concurrent task.
            tokio::spawn(handle_conn)
        });

    // Start the Tokio runtime
    tokio::run(server);
}
```

More examples can be found [here](examples).

## Getting Help

First, see if the answer to your question can be found in the [Guides] or the
[API documentation]. If the answer is not there, there is an active community in
the [Tokio Gitter channel][chat]. We would be happy to try to answer your
question.  Last, if that doesn't work, try opening an [issue] with the question.

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

* [`tokio-async-await`]: Experimental `async` / `await` support.

* [`tokio-codec`]: Utilities for encoding and decoding protocol frames.

* [`tokio-current-thread`]: Schedule the execution of futures on the current
  thread.

* [`tokio-executor`]: Task execution related traits and utilities.

* [`tokio-fs`]: Filesystem (and standard in / out) APIs.

* [`tokio-io`]: Asynchronous I/O related traits and utilities.

* [`tokio-reactor`]: Event loop that drives I/O resources (like TCP and UDP
  sockets).

* [`tokio-tcp`]: TCP bindings for use with `tokio-io` and `tokio-reactor`.

* [`tokio-threadpool`]: Schedules the execution of futures across a pool of
  threads.

* [ `tokio-timer`]: Time related APIs.

* [`tokio-udp`]: UDP bindings for use with `tokio-io` and `tokio-reactor`.

* [`tokio-uds`]: Unix Domain Socket bindings for use with `tokio-io` and
  `tokio-reactor`.

[`tokio-async-await`]: tokio-async-await
[`tokio-codec`]: tokio-codec
[`tokio-current-thread`]: tokio-current-thread
[`tokio-executor`]: tokio-executor
[`tokio-fs`]: tokio-fs
[`tokio-io`]: tokio-io
[`tokio-reactor`]: tokio-reactor
[`tokio-tcp`]: tokio-tcp
[`tokio-threadpool`]: tokio-threadpool
[`tokio-timer`]: tokio-timer
[`tokio-udp`]: tokio-udp
[`tokio-uds`]: tokio-uds

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
