# tokio-process

An implementation of process management for Tokio

> This crate has been **deprecated in tokio 0.2.x** and has been moved into
> [`tokio::process`] behind the `process` [feature flag].

[`tokio::process`]: https://docs.rs/tokio/latest/tokio/process/index.html
[feature flag]: https://docs.rs/tokio/latest/tokio/index.html#feature-flags

[Documentation](https://docs.rs/tokio-process/0.2.4/tokio_process)

## Usage

First, add this to your `Cargo.toml`:

```toml
[dependencies]
tokio-process = "0.2"
```

Next you can use this in conjunction with the `tokio` and `futures` crates:

```rust,no_run
use std::process::Command;

use futures::Future;
use tokio_process::CommandExt;

fn main() {
    // Use the standard library's `Command` type to build a process and
    // then execute it via the `CommandExt` trait.
    let child = Command::new("echo").arg("hello").arg("world").spawn_async();

    // Make sure our child succeeded in spawning and process the result
    let future = child
        .expect("failed to spawn")
        .map(|status| println!("exit status: {}", status))
        .map_err(|e| panic!("failed to wait for exit: {}", e));

    // Send the future to the tokio runtime for execution
    tokio::run(future)
}
```

## License

This project is licensed under the [MIT license](./LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tokio by you, shall be licensed as MIT, without any additional
terms or conditions.
