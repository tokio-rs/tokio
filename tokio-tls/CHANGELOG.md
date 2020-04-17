# 0.3.0 (November 26, 2019)

- Updates for tokio 0.2 release

# 0.3.0-alpha.6 (September 30, 2019)

- Move to `futures-*-preview 0.3.0-alpha.19`
- Move to `pin-project 0.4`

# 0.3.0-alpha.5 (September 19, 2019)

### Added
- `TlsStream::get_ref` and `TlsStream::get_mut` ([#1537]).

# 0.3.0-alpha.4 (August 30, 2019)

### Changed
- Track `tokio` 0.2.0-alpha.4

# 0.3.0-alpha.2 (August 17, 2019)

### Changed
- Update `futures` dependency to 0.3.0-alpha.18.

# 0.3.0-alpha.1 (August 8, 2019)

### Changed
- Switch to `async`, `await`, and `std::future`.

# 0.2.1 (January 6, 2019)

* Implement `Clone` for `TlsConnector` and `TlsAcceptor` ([#777])

# 0.2.0 (August 8, 2018)

* Initial release with `tokio` support.

[#1537]: https://github.com/tokio-rs/tokio/pull/1537
[#777]:  https://github.com/tokio-rs/tokio/pull/777
