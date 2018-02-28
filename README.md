# Tokio

A runtime for writing reliable, asynchronous, and slim applications with
the Rust programming langauge. It is:

* **Fast**: Tokio's zero-cost abstractions give you bare-metal
  performance.

* **Reliable**: Tokio leverages Rust's ownership, type system, and
  concurrency model to reduce bugs and ensure thread safety.

* **Scalable**: Tokio has a minimal footprint, and handles backpressure
  and cancellation naturally.

[![Travis Build Status][travis-badge]][travis-url]
[![Appveyor Build Status][appveyor-badge]][appveyor-url]

[travis-badge]: https://travis-ci.org/tokio-rs/tokio.svg?branch=master
[travis-url]: https://travis-ci.org/tokio-rs/tokio
[appveyor-badge]: https://ci.appveyor.com/api/projects/status/s83yxhy9qeb58va7?svg=true
[appveyor-url]: https://ci.appveyor.com/project/carllerche/tokio/branch/master

[Website](https://tokio.rs) |
[Guides](https://tokio.rs/docs/getting-started/hello-world/) |
[API Docs](https://docs.rs/tokio)

## Overview

Tokio is an event-driven, non-blocking I/O platform for writing
asynchronous applications with the Rust programming language. At a high
level, it provides a few major components:

* A multi threaded, work-stealing based task [scheduler].
* A [reactor] backed by the operating system's event queue (epoll, kqueue,
  IOCP, etc...).
* Asynchronous [TCP and UDP][net] sockets.

These components provide the runtime components necessary for building
an asynchronous application.

[net]: https://docs.rs/tokio/0.1/tokio/net/index.html
[reactor]: https://docs.rs/tokio/0.1.1/tokio/reactor/index.html
[scheduler]: https://tokio-rs.github.io/tokio/tokio/runtime/index.html

# License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in tokio by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
