# 0.3.1 (March 18, 2020)

### Fixed

- Adjust minimum-supported Tokio version to v0.2.5 to account for an internal
  dependency on features in that version of Tokio. (#2326)

# 0.3.0 (March 4, 2020)

### Changed

- **Breaking Change**: Change `Encoder` trait to take a generic `Item` parameter, which allows
  codec writers to pass references into `Framed` and `FramedWrite` types. (#1746)

### Added

- Add futures-io/tokio::io compatibility layer. (#2117)
- Add `Framed::with_capacity`. (#2215)

### Fixed

- Use advance over split_to when data is not needed. (#2198)

# 0.2.0 (November 26, 2019)

- Initial release
