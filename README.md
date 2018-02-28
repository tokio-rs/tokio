# Tokio

A platform for writing fast networking code with Rust.

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

Tokio is an event-driven, non-blocking I/O platform for writing asynchronous I/O
backed applications. It is used for implementing networking clients and servers.

Tokio uses the [`futures`] crate as a foundation to build on, providing
networking types and other utilities needed for building a production ready
application.

[`futures`]: https://github.com/rust-lang-nursery/futures-rs

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
