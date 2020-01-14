# tokio-codec

Utilities for encoding and decoding frames.

> **Note:** This crate is **deprecated in tokio 0.2.x** and has been moved into
> [`tokio_util::codec`] of the [`tokio-util` crate] behind the `codec` feature
> flag.

[`tokio_util::codec`]: https://docs.rs/tokio-util/latest/tokio_util/codec/index.html
[`tokio-util` crate]: https://docs.rs/tokio-util/latest/tokio_util

[Documentation](https://docs.rs/tokio-codec)

## Usage

First, add this to your `Cargo.toml`:

```toml
[dependencies]
tokio-codec = "0.1"
```

Next, add this to your crate:

```rust
extern crate tokio_codec;
```

You can find extensive documentation and examples about how to use this crate
online at [https://tokio.rs](https://tokio.rs). The [API
documentation](https://docs.rs/tokio-codec) is also a great place to get started
for the nitty-gritty.

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tokio by you, shall be licensed as MIT, without any additional
terms or conditions.
