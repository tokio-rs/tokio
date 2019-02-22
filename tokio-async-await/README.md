# Tokio async/await preview

This crate provides a preview of Tokio with async / await support. It is a shim
layer on top of `tokio`.

**This crate requires Rust nightly and does not provide API stability
guarantees. You are living on the edge here.**

## Usage

To use this crate, you need to start with a Rust 2018 edition crate, with rustc
1.34.0-nightly or later.

Add this to your `Cargo.toml`:

```toml
# In the `[packages]` section
edition = "2018"

# In the `[dependencies]` section
tokio = {version = "0.1.0", features = ["async-await-preview"]}
```

Then, get started. In your application, add:

```rust
// The nightly features that are commonly needed with async / await
#![feature(await_macro, async_await, futures_api)]

// This pulls in the `tokio-async-await` crate. While Rust 2018 doesn't require
// `extern crate`, we need to pull in the macros.
#[macro_use]
extern crate tokio;

fn main() {
    // And we are async...
    tokio::run_async(async {
        println!("Hello");
    });
}
```

Because nightly is required, run the app with `cargo +nightly run`

Check the [examples](examples) directory for more.

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tokio by you, shall be licensed as MIT, without any additional
terms or conditions.
