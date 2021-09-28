# 0.1.7 (July 7, 2021)

### Fixed

- sync: fix watch wrapper ([#3914])
- time: fix `Timeout::size_hint` ([#3902])

[#3902]: https://github.com/tokio-rs/tokio/pull/3902
[#3914]: https://github.com/tokio-rs/tokio/pull/3914

# 0.1.6 (May 14, 2021)

### Added

- stream: implement `Error` and `Display` for `BroadcastStreamRecvError` ([#3745])

### Fixed

- stream: avoid yielding in `AllFuture` and `AnyFuture` ([#3625])

[#3745]: https://github.com/tokio-rs/tokio/pull/3745
[#3625]: https://github.com/tokio-rs/tokio/pull/3625

# 0.1.5 (March 20, 2021)

### Fixed

- stream: documentation note for throttle `Unpin` ([#3600])

[#3600]: https://github.com/tokio-rs/tokio/pull/3600

# 0.1.4 (March 9, 2021)

Added

- signal: add `Signal` wrapper ([#3510])

Fixed

- stream: remove duplicate `doc_cfg` declaration ([#3561])
- sync: yield initial value in `WatchStream` ([#3576])

[#3510]: https://github.com/tokio-rs/tokio/pull/3510
[#3561]: https://github.com/tokio-rs/tokio/pull/3561
[#3576]: https://github.com/tokio-rs/tokio/pull/3576

# 0.1.3 (February 5, 2021)

Added

 - sync: add wrapper for broadcast and watch ([#3384], [#3504])

[#3384]: https://github.com/tokio-rs/tokio/pull/3384
[#3504]: https://github.com/tokio-rs/tokio/pull/3504

# 0.1.2 (January 12, 2021)

Fixed

 - docs: fix some wrappers missing in documentation ([#3378])

[#3378]: https://github.com/tokio-rs/tokio/pull/3378

# 0.1.1 (January 4, 2021)

Added

 - add `Stream` wrappers ([#3343])

Fixed

 - move `async-stream` to `dev-dependencies` ([#3366])

[#3366]: https://github.com/tokio-rs/tokio/pull/3366
[#3343]: https://github.com/tokio-rs/tokio/pull/3343

# 0.1.0 (December 23, 2020)

 - Initial release
