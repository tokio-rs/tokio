# 0.6.3 (January 31, 2021)

### Added

- sync: add `ReusableBoxFuture` utility ([#3464])

### Changed

- sync: use `ReusableBoxFuture` for `PollSemaphore` ([#3463])
- deps: remove `async-stream` dependency ([#3463])
- deps: remove `tokio-stream` dependency ([#3487])

# 0.6.2 (January 21, 2021)

### Added

- sync: add pollable `Semaphore` ([#3444])

### Fixed

- time: fix panics on updating `DelayQueue` entries ([#3270])

# 0.6.1 (January 12, 2021)

### Added

- codec: `get_ref()`, `get_mut()`, `get_pin_mut()` and `into_inner()` for
  `Framed`, `FramedRead`, `FramedWrite` and `StreamReader` ([#3364]).
- codec: `write_buffer()` and `write_buffer_mut()` for `Framed` and
  `FramedWrite` ([#3387]).

# 0.6.0 (December 23, 2020)

### Changed
- depend on `tokio` 1.0.

### Added
- rt: add constructors to `TokioContext` (#3221).

# 0.5.1 (December 3, 2020)

### Added
- io: `poll_read_buf` util fn (#2972).
- io: `poll_write_buf` util fn with vectored write support (#3156).

# 0.5.0 (October 30, 2020)

### Changed
- io: update `bytes` to 0.6 (#3071).

# 0.4.0 (October 15, 2020)

### Added
- sync: `CancellationToken` for coordinating task cancellation (#2747).
- rt: `TokioContext` sets the Tokio runtime for the duration of a future (#2791)
- io: `StreamReader`/`ReaderStream` map between `AsyncRead` values and `Stream`
  of bytes (#2788).
- time: `DelayQueue` to manage many delays (#2897).

# 0.3.1 (March 18, 2020)

### Fixed

- Adjust minimum-supported Tokio version to v0.2.5 to account for an internal
  dependency on features in that version of Tokio. ([#2326])

# 0.3.0 (March 4, 2020)

### Changed

- **Breaking Change**: Change `Encoder` trait to take a generic `Item` parameter, which allows
  codec writers to pass references into `Framed` and `FramedWrite` types. ([#1746])

### Added

- Add futures-io/tokio::io compatibility layer. ([#2117])
- Add `Framed::with_capacity`. ([#2215])

### Fixed

- Use advance over split_to when data is not needed. ([#2198])

# 0.2.0 (November 26, 2019)

- Initial release

[#3487]: https://github.com/tokio-rs/tokio/pull/3487
[#3464]: https://github.com/tokio-rs/tokio/pull/3464
[#3463]: https://github.com/tokio-rs/tokio/pull/3463
[#3444]: https://github.com/tokio-rs/tokio/pull/3444
[#3387]: https://github.com/tokio-rs/tokio/pull/3387
[#3364]: https://github.com/tokio-rs/tokio/pull/3364
[#3270]: https://github.com/tokio-rs/tokio/pull/3270
[#2326]: https://github.com/tokio-rs/tokio/pull/2326
[#2215]: https://github.com/tokio-rs/tokio/pull/2215
[#2198]: https://github.com/tokio-rs/tokio/pull/2198
[#2117]: https://github.com/tokio-rs/tokio/pull/2117
[#1746]: https://github.com/tokio-rs/tokio/pull/1746
