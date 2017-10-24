# tokio-core

Core I/O and event loop abstraction for asynchronous I/O in Rust built on
`futures` and `mio`.

[![Build Status](https://travis-ci.org/tokio-rs/tokio-core.svg?branch=master)](https://travis-ci.org/tokio-rs/tokio-core)
[![Build status](https://ci.appveyor.com/api/projects/status/caxmxbg8181kk9mq/branch/master?svg=true)](https://ci.appveyor.com/project/carllerche/tokio-core)

[Documentation](https://docs.rs/tokio-core)

[Tutorial](https://tokio.rs/)

## Usage

First, add this to your `Cargo.toml`:

```toml
[dependencies]
tokio-core = "0.1"
```

Next, add this to your crate:

```rust
extern crate tokio_core;
```

You can find extensive documentation and examples about how to use this crate
online at [https://tokio.rs](https://tokio.rs) as well as the `examples` folder
in this repository. The [API documentation](https://docs.rs/tokio-core) is also
a great place to get started for the nitty-gritty.

# License

`tokio-core` is primarily distributed under the terms of both the MIT license
and the Apache License (Version 2.0), with portions covered by various BSD-like
licenses.

See LICENSE-APACHE, and LICENSE-MIT for details.
