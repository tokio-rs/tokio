# Next Release

### Breaking Change
- Removed `Callsite::clear_interest` and `Callsite::add_interest` (#1039)

### Added
- Add a function to rebuild cached interest (#1039)
- Add overrideable downcasting to `Subscriber`s (#974)
- Add slightly more useful debug impls (#1014)

### Fixed
- `fmt::Debug` impls for `field::Display` and `field::Debug` not passing through
  to the inner value (#992)

# 0.1.0 (March 13, 2019)

- Initial release
