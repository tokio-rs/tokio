# 0.2.0 (April 21, 2019)

### Breaking Changes
- Remove `Callsite::clear_interest` and `Callsite::add_interest` (#1039)
- `metadata!` macro now requires a `Kind` field (#1046)

### Added
- Add a function to rebuild cached interest (#1039)
- Add overrideable downcasting to `Subscriber`s (#974)
- Add slightly more useful debug impls (#1014)
- Introduce callsite classification in metadata (#1046)

### Fixed
- `fmt::Debug` impls for `field::Display` and `field::Debug` not passing through
  to the inner value (#992)
- Entering a `Dispatch` function unsets the default dispatcher for the duration
  of the function (so that events inside the subscriber cannot cause infinite
  loops) (#1033)

# 0.1.0 (March 13, 2019)

- Initial release
