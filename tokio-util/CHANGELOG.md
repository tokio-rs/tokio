### Added
- io: `poll_read_buf` util fn (#2972).

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

[#2326]: https://github.com/tokio-rs/tokio/pull/2326
[#2215]: https://github.com/tokio-rs/tokio/pull/2215
[#2198]: https://github.com/tokio-rs/tokio/pull/2198
[#2117]: https://github.com/tokio-rs/tokio/pull/2117
[#1746]: https://github.com/tokio-rs/tokio/pull/1746
