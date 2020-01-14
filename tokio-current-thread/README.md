# tokio-current-thread

Single threaded executor for Tokio.

> **Note:** This crate is **deprecated in tokio 0.2.x** and has been moved and
> refactored into various places in the [`tokio`] crate. The closest replacement
> is to make use of [`tokio::task::LocalSet::block_on`] which requires the
> [`rt-util` feature].

[`tokio`]: https://docs.rs/tokio/latest/tokio/index.html
[`tokio::task::LocalSet::block_on`]: https://docs.rs/tokio/latest/tokio/task/struct.LocalSet.html#method.block_on
[`rt-util` feature]: https://docs.rs/tokio/latest/tokio/index.html#feature-flags

[Documentation](https://docs.rs/tokio-current-thread/0.1.6/tokio_current_thread/)

## Overview

This crate provides the single threaded executor which execute many tasks concurrently.

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tokio by you, shall be licensed as MIT, without any additional
terms or conditions.
