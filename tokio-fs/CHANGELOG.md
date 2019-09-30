# 0.2.0-alpha.6 (September 30, 2019)

- Move to `futures-*-preview 0.3.0-alpha.19`
- Move to `pin-project 0.4`

# 0.2.0-alpha.5 (September 19, 2019)

### Fix
- propagate flush for stdout / stderr. (#1528).

# 0.2.0-alpha.4 (August 29, 2019)

- Track tokio release.

# 0.2.0-alpha.3 (August 28, 2019)

### Changed
- Temporarily use a dedicated thread pool for all blocking operations (#1495).

# 0.2.0-alpha.2 (August 17, 2019)

### Changed
- Update `futures` dependency to 0.3.0-alpha.18.

# 0.2.0-alpha.1 (August 8, 2019)

### Changed
- Switch to `async`, `await`, and `std::future`.

# 0.1.6 (March 1, 2019)

### Added
- File::try_clone() (#850).
- Async equivalent of read / write file helpers being added to `std` (#896).

# 0.1.5 (January 6, 2019)

* Add examples to `File` API docs (#786).

# 0.1.4 (October 23, 2018)

* Provide `File::from_std` (#696).

# 0.1.3 (August 6, 2018)

* Add async equivalents to most of `std::fs` (#494).

# 0.1.2 (July 11, 2018)

* Add `metadata` and `File::metadata` ([#433](https://github.com/tokio-rs/tokio/pull/433), [#385](https://github.com/tokio-rs/tokio/pull/385))
* Add `File::seek` ([#434](https://github.com/tokio-rs/tokio/pull/434))

# 0.1.1 (June 13, 2018)

* Add `OpenOptions` ([#390](https://github.com/tokio-rs/tokio/pull/390))
* Add `into_std` to `File` ([#403](https://github.com/tokio-rs/tokio/pull/403))
* Use `tokio-codec` in examples

# 0.1.0 (May 2, 2018)

* Initial release
