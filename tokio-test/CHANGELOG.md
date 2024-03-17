# 0.4.4 (March 14, 2024)

- task: mark `Spawn` as `#[must_use]` ([#6371])
- test: increase MSRV to 1.63 ([#6126])
- test: update category slug ([#5953])

[#5953]: https://github.com/tokio-rs/tokio/pull/5953
[#6126]: https://github.com/tokio-rs/tokio/pull/6126
[#6371]: https://github.com/tokio-rs/tokio/pull/6371

# 0.4.3 (August 23, 2023)

- deps: fix minimum required version of `async-stream` ([#5347])
- deps: fix minimum required version of `tokio-stream` ([#4376])
- docs: improve `tokio_test::task` docs ([#5132])
- io: fetch actions from mock handle before write ([#5814])
- io: fix wait operation on mock ([#5554])

[#4376]: https://github.com/tokio-rs/tokio/pull/4376
[#5132]: https://github.com/tokio-rs/tokio/pull/5132
[#5347]: https://github.com/tokio-rs/tokio/pull/5347
[#5554]: https://github.com/tokio-rs/tokio/pull/5554
[#5814]: https://github.com/tokio-rs/tokio/pull/5814

# 0.4.2 (May 14, 2021)

- test: add `assert_elapsed!` macro ([#3728])

[#3728]: https://github.com/tokio-rs/tokio/pull/3728

# 0.4.1 (March 10, 2021)

- Fix `io::Mock` to be `Send` and `Sync` ([#3594])

[#3594]: https://github.com/tokio-rs/tokio/pull/3594

# 0.4.0 (December 23, 2020)

- Track `tokio` 1.0 release.

# 0.3.0 (October 15, 2020)

- Track `tokio` 0.3 release.

# 0.2.1 (April 17, 2020)

- Add `Future` and `Stream` implementations for `task::Spawn<T>`.

# 0.2.0 (November 25, 2019)

- Initial release
