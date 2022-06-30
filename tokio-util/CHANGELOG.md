# 0.7.3 (June 4, 2022)

### Changed

- tracing: don't require default tracing features ([#4592])
- util: simplify implementation of `ReusableBoxFuture` ([#4675])

### Added (unstable)

- task: add `JoinMap` ([#4640], [#4697])

[#4592]: https://github.com/tokio-rs/tokio/pull/4592
[#4640]: https://github.com/tokio-rs/tokio/pull/4640
[#4675]: https://github.com/tokio-rs/tokio/pull/4675
[#4697]: https://github.com/tokio-rs/tokio/pull/4697

# 0.7.2 (May 14, 2022)

This release contains a rewrite of `CancellationToken` that fixes a memory leak. ([#4652])

[#4652]: https://github.com/tokio-rs/tokio/pull/4652

# 0.7.1 (February 21, 2022)

### Added

- codec: add `length_field_type` to `LengthDelimitedCodec` builder ([#4508])
- io: add `StreamReader::into_inner_with_chunk()` ([#4559])

### Changed

- switch from log to tracing ([#4539])

### Fixed

- sync: fix waker update condition in `CancellationToken` ([#4497])
- bumped tokio dependency to 1.6 to satisfy minimum requirements ([#4490])

[#4490]: https://github.com/tokio-rs/tokio/pull/4490
[#4497]: https://github.com/tokio-rs/tokio/pull/4497
[#4508]: https://github.com/tokio-rs/tokio/pull/4508
[#4539]: https://github.com/tokio-rs/tokio/pull/4539
[#4559]: https://github.com/tokio-rs/tokio/pull/4559

# 0.7.0 (February 9, 2022)

### Added

- task: add `spawn_pinned` ([#3370])
- time: add `shrink_to_fit` and `compact` methods to `DelayQueue` ([#4170])
- codec: improve `Builder::max_frame_length` docs ([#4352])
- codec: add mutable reference getters for codecs to pinned `Framed` ([#4372])
- net: add generic trait to combine `UnixListener` and `TcpListener` ([#4385])
- codec: implement `Framed::map_codec` ([#4427])
- codec: implement `Encoder<BytesMut>` for `BytesCodec` ([#4465])

### Changed

- sync: add lifetime parameter to `ReusableBoxFuture` ([#3762])
- sync: refactored `PollSender<T>` to fix a subtly broken `Sink<T>` implementation ([#4214])
- time: remove error case from the infallible `DelayQueue::poll_elapsed` ([#4241])

[#3370]: https://github.com/tokio-rs/tokio/pull/3370
[#4170]: https://github.com/tokio-rs/tokio/pull/4170
[#4352]: https://github.com/tokio-rs/tokio/pull/4352
[#4372]: https://github.com/tokio-rs/tokio/pull/4372
[#4385]: https://github.com/tokio-rs/tokio/pull/4385
[#4427]: https://github.com/tokio-rs/tokio/pull/4427
[#4465]: https://github.com/tokio-rs/tokio/pull/4465
[#3762]: https://github.com/tokio-rs/tokio/pull/3762
[#4214]: https://github.com/tokio-rs/tokio/pull/4214
[#4241]: https://github.com/tokio-rs/tokio/pull/4241

# 0.6.10 (May 14, 2021)

This is a backport for the memory leak in `CancellationToken` that was originally fixed in 0.7.2. ([#4652])

[#4652]: https://github.com/tokio-rs/tokio/pull/4652

# 0.6.9 (October 29, 2021)

### Added

- codec: implement `Clone` for `LengthDelimitedCodec` ([#4089])
- io: add `SyncIoBridge`  ([#4146])

### Fixed

- time: update deadline on removal in `DelayQueue` ([#4178])
- codec: Update stream impl for Framed to return None after Err ([#4166])

[#4089]: https://github.com/tokio-rs/tokio/pull/4089
[#4146]: https://github.com/tokio-rs/tokio/pull/4146
[#4166]: https://github.com/tokio-rs/tokio/pull/4166
[#4178]: https://github.com/tokio-rs/tokio/pull/4178

# 0.6.8 (September 3, 2021)

### Added

- sync: add drop guard for `CancellationToken` ([#3839])
- compact: added `AsyncSeek` compat ([#4078])
- time: expose `Key` used in `DelayQueue`'s `Expired` ([#4081])
- io: add `with_capacity` to `ReaderStream` ([#4086])

### Fixed

- codec: remove unnecessary `doc(cfg(...))` ([#3989])

[#3839]: https://github.com/tokio-rs/tokio/pull/3839
[#4078]: https://github.com/tokio-rs/tokio/pull/4078
[#4081]: https://github.com/tokio-rs/tokio/pull/4081
[#4086]: https://github.com/tokio-rs/tokio/pull/4086
[#3989]: https://github.com/tokio-rs/tokio/pull/3989

# 0.6.7 (May 14, 2021)

### Added

- udp: make `UdpFramed` take `Borrow<UdpSocket>` ([#3451])
- compat: implement `AsRawFd`/`AsRawHandle` for `Compat<T>` ([#3765])

[#3451]: https://github.com/tokio-rs/tokio/pull/3451
[#3765]: https://github.com/tokio-rs/tokio/pull/3765

# 0.6.6 (April 12, 2021)

### Added

- util: makes `Framed` and `FramedStream` resumable after eof ([#3272])
- util: add `PollSemaphore::{add_permits, available_permits}` ([#3683])

### Fixed

- chore: avoid allocation if `PollSemaphore` is unused ([#3634])

[#3272]: https://github.com/tokio-rs/tokio/pull/3272
[#3634]: https://github.com/tokio-rs/tokio/pull/3634
[#3683]: https://github.com/tokio-rs/tokio/pull/3683

# 0.6.5 (March 20, 2021)

### Fixed

- util: annotate time module as requiring `time` feature ([#3606])

[#3606]: https://github.com/tokio-rs/tokio/pull/3606

# 0.6.4 (March 9, 2021)

### Added

- codec: `AnyDelimiter` codec ([#3406])
- sync: add pollable `mpsc::Sender` ([#3490])

### Fixed

- codec: `LinesCodec` should only return `MaxLineLengthExceeded` once per line ([#3556])
- sync: fuse PollSemaphore ([#3578])

[#3406]: https://github.com/tokio-rs/tokio/pull/3406
[#3490]: https://github.com/tokio-rs/tokio/pull/3490
[#3556]: https://github.com/tokio-rs/tokio/pull/3556
[#3578]: https://github.com/tokio-rs/tokio/pull/3578

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
