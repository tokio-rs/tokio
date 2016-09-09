# Tokio

Tokio is a one-stop-shop for all your async I/O needs in Rust. At the heart of
Tokio is the [`futures`] crate to model asynchronous computations, and you'll
find all layers from futures to event loops to protocols to services in the
various Tokio crates.

> **Note**: Currently the Tokio project is a work in progress, so breakage
>           should be expected. Crates will be published to crates.io as they
>           begin to stabilize, and this repository itself will eventually be
>           home to a `tokio` crate as well. Stay tuned for updates!

## What is Tokio?

Tokio is a an async I/O stack in Rust running the full gamut in terms of what it
provides. At the very bottom you'll find the [`tokio-core`] crate with a
bare-bones event loop and `Future` spawning. Near the top you'll find the
[`tokio-proto`] crate with generic implementations of protocol details like
pipelining and multiplexing used to serve requests to the [`Service`] trait in
[`tokio-service`] inspired by [Finagle].

[Finagle]: https://twitter.github.io/finagle/

The Tokio project is split into a number of crates to encapsulate
responsibilities and encourage reuse. Some layers of the Tokio stack are more
"opinionated" than others, for example with a particular buffering strategy.
Currently the lowest layers, [`tokio-core`] and [`tokio-service`], are intended
to be zero-to-very-low cost abstractions while moving up to crates like
[`tokio-proto`] may not be zero cost for some applications.

It's intended that the `tokio` crate (not existent yet) is generally what you'll
want for async I/O, and more specialized applications can use the dependencies
individually. This document is then intended to help you navigate what's
available!

## Tokio Crates

* [`tokio-core`] - the core I/O event loop for all of Tokio
* [`tokio-service`] - definition of the `Service` trait for applications wishing
  to define a service or work with middleware.
* [`tokio-proto`] - protocol implementation utilities such as
  pipelining/multiplexing
* [`tokio-curl`] - a sample HTTP client backed by libcurl
* [`tokio-socks5`] - a sample SOCKSv5 proxy server
* [`tokio-line`] - a sample line-based protocol for using [`tokio-proto`]
* [`tokio-middleware`] - collection of middleware using the [`tokio-service`] crate
* [`tokio-minihttp`] - a mini HTTP server implementation intended to showcase
  [`tokio-proto`]
* [`tokio-redis`] - an implementation of the Redis protocol using
  [`tokio-proto`]
* [`tokio-timer`] - a timer wheel and other timer-related functions for use with
  an event loop
* [`tokio-tls`] - an implementation of TLS/SSL streams used by
  [`tokio-minihttp`] for an SSL server
* [`tokio-uds`] - Unix domain sockets for [`tokio-core`]

### Third-party Tokio Crates

In addition to the crates above in the `tokio-rs` organization, there are a few
crates outside the organization which integrate with Tokio:

* [`tokio-dns`] - asynchronous DNS resolution and utilities
* [`tokio-signal`] - Unix signal handling and Windows console control events
* [`tokio-process`] - process management and spawning

## Current Status

A bottom-up approach is currently being used to stabilize pieces and enable more
and more applications to build on the stable portions of Tokio. The [`futures`]
crate recently published 0.1, and the Tokio crates are intended to follow suit
soon. Listed below is a table of the current status of each crate.

|  Crate               |   Status |
|----------------------|----------|
| [`tokio-core`]       | [Nearly 0.1](https://github.com/tokio-rs/tokio-core/milestone/1) |
| [`tokio-service`]    | Nearly 0.1 |
| [`tokio-proto`]      | Waiting on `tokio-core`, next major piece |
| [`tokio-curl`]       | Waiting on `tokio-core` |
| [`tokio-timer`]      | Waiting on `tokio-core` |
| [`tokio-tls`]        | Waiting on `tokio-core` and `native-tls` |
| [`tokio-uds`]        | Waiting on `tokio-core` |
| [`tokio-middleware`] | Planned soon after `tokio-service` |

Note that the following example projects are not currently intended to be
published on crates.io:

* [`tokio-socks5`]
* [`tokio-line`]
* [`tokio-minihttp`]
* [`tokio-redis`]

## License

Tokio is primarily distributed under the terms of both the MIT license
and the Apache License (Version 2.0), with portions covered by various
BSD-like licenses.

See LICENSE-APACHE, and LICENSE-MIT for details.

[`tokio-core`]: https://github.com/tokio-rs/tokio-core
[`tokio-curl`]: https://github.com/tokio-rs/tokio-curl
[`tokio-dns`]: https://github.com/sbstp/tokio-dns
[`tokio-line`]: https://github.com/tokio-rs/tokio-line
[`tokio-middleware`]: https://github.com/tokio-rs/tokio-middleware
[`tokio-minihttp`]: https://github.com/tokio-rs/tokio-minihttp
[`tokio-process`]: https://github.com/alexcrichton/tokio-process
[`tokio-proto`]: https://github.com/tokio-rs/tokio-proto
[`tokio-redis`]: https://github.com/tokio-rs/tokio-redis
[`tokio-service`]: https://github.com/tokio-rs/tokio-service
[`tokio-signal`]: https://github.com/alexcrichton/tokio-signal
[`tokio-socks5`]: https://github.com/tokio-rs/tokio-socks5
[`tokio-timer`]: https://github.com/tokio-rs/tokio-timer
[`tokio-tls`]: https://github.com/tokio-rs/tokio-tls
[`tokio-uds`]: https://github.com/tokio-rs/tokio-uds
[`futures`]: https://github.com/alexcrichton/futures-rs
[`native-tls`]: https://github.com/sfackler/rust-native-tls
[`Service`]: https://tokio-rs.github.io/tokio-service/tokio_service/trait.Service.html
