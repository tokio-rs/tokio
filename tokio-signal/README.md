# tokio-signal

Unix signal handling for Tokio.

[Documentation](https://docs.rs/tokio-signal/0.2.8/tokio_signal)

## Usage

First, add this to your `Cargo.toml`:

```toml
[dependencies]
tokio-signal = "0.2.8"
```

Next you can use this in conjunction with the `tokio` and `futures` crates:

```rust,no_run
use futures::{Future, Stream};

fn main() {

    // Create an infinite stream of "Ctrl+C" notifications. Each item received
    // on this stream may represent multiple ctrl-c signals.
    let ctrl_c = tokio_signal::ctrl_c().flatten_stream();

    // Process each ctrl-c as it comes in
    let prog = ctrl_c.for_each(|()| {
        println!("ctrl-c received!");
        Ok(())
    });

    tokio::run(prog.map_err(|err| panic!("{}", err)));
}
```

## License

This project is licensed under the [MIT license](./LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tokio by you, shall be licensed as MIT, without any additional
terms or conditions.
