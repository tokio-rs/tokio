# Tokio Compat

Compatibility layers between `tokio` 0.2 and legacy versions.

[![Crates.io][crates-badge]][crates-url]
[![MIT licensed][mit-badge]][mit-url]
[![Build Status][azure-badge]][azure-url]
[![Gitter chat][gitter-badge]][gitter-url]

[crates-badge]: https://img.shields.io/crates/v/tokio-compat.svg
[crates-url]: https://crates.io/crates/tokio-compat
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: LICENSE
[azure-badge]: https://dev.azure.com/tokio-rs/Tokio/_apis/build/status/tokio-rs.tokio?branchName=master
[azure-url]: https://dev.azure.com/tokio-rs/Tokio/_build/latest?definitionId=1&branchName=master
[gitter-badge]: https://img.shields.io/gitter/room/tokio-rs/tokio.svg
[gitter-url]: https://gitter.im/tokio-rs/tokio

[Website](https://tokio.rs) |
[Guides](https://tokio.rs/docs/) |
[API Docs](https://docs.rs/tokio-compat/0.1.0-alpha.1/tokio-compat) |
[Chat](https://gitter.im/tokio-rs/tokio)

## Overview

This crate provides compatibility runtimes that allow running both `futures` 0.1
futures that use `tokio` 0.1 runtime services _and_ `std::future` futures that
use `tokio` 0.2 runtime services.

### Examples

Spawning both `tokio` 0.1 and `tokio` 0.2 futures:

```rust
use futures_01::future::lazy;

tokio_compat::run(lazy(|| {
    // spawn a `futures` 0.1 future using the `spawn` function from the
    // `tokio` 0.1 crate:
    tokio_01::spawn(lazy(|| {
        println!("hello from tokio 0.1!");
        Ok(())
    }));

    // spawn an `async` block future on the same runtime using `tokio`
    // 0.2's `spawn`:
    tokio_02::spawn(async {
        println!("hello from tokio 0.2!");
    });

    Ok(())
}))
```

Futures on the compat runtime can use `timer` APIs from both 0.1 and 0.2
versions of `tokio`:

```rust
use std::time::{Duration, Instant};
use futures_01::future::lazy;
use tokio_compat::prelude::*;

tokio_compat::run_std(async {
    // Wait for a `tokio` 0.1 `Delay`...
    let when = Instant::now() + Duration::from_millis(10);
    tokio_01::timer::Delay::new(when)
        // convert the delay future into a `std::future` that we can `await`.
        .compat()
        .await
        .expect("tokio 0.1 timer should work!");
    println!("10 ms have elapsed");

    // Wait for a `tokio` 0.2 `Delay`...
    let when = Instant::now() + Duration::from_millis(20);
    tokio_02::timer::delay(when).await;
    println!("20 ms have elapsed");
});
```

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tokio by you, shall be licensed as MIT, without any additional
terms or conditions.
