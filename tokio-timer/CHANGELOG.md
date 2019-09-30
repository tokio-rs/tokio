# 0.3.0-alpha.6 (September 30, 2019)

- Move to `futures-*-preview 0.3.0-alpha.19`
- Move to `pin-project 0.4`

# 0.3.0-alpha.5 (September 19, 2019)

### Changed
- rename `sleep` to `delay_for` (#1518).

# 0.3.0-alpha.4 (August 29, 2019)

- Track tokio release.

# 0.3.0-alpha.3 (August 28, 2019)

### Changed
- `delay(...)` instead of `Delay::new(...)` (#1440).

# 0.3.0-alpha.2 (August 17, 2019)

### Changed
- Update `futures` dependency to 0.3.0-alpha.18.
- Switch `with_default(..., || )` to `set_default(...) -> Guard` (#1449).

# 0.3.0-alpha.1 (August 8, 2019)

### Changed
- Switch to `async`, `await`, and `std::future`.

# 0.2.11 (May 14, 2019)

### Added
- `Handle::timeout` API, replacing the deprecated `Handle::deadline` (#1074).

# 0.2.10 (February 4, 2019)

### Fixed
- `DelayQueue` when multiple delays are reset (#871).

# 0.2.9 (January 24, 2019)

### Fixed
- `DelayQueue` timing logic when inserting / resetting a delay (#851, #863).
- Documentation links (#842, #844, #845)

# 0.2.8 (November 21, 2018)

* Implement throttle combinator (#736).
* Derive `Clone` for `delay_queue::Key` (#730).
* Bump internal dependencies (#753).

# 0.2.7 (September 27, 2018)

* Fix `Timeout` on error bug (#648).
* Miscellaneous documentation improvements.

# 0.2.6 (August 23, 2018)

* Implement `Default` for `timer::Handle` (#553)
* Provide `DelayQueue` utility (#550)
* Reduce size of `Delay` struct (#554)
* Introduce `Timeout`, deprecate `Deadline` (#558)

# 0.2.5 (August 6, 2018)

* Add `Interval::interval` shortcut (#492).

# 0.2.4 (June 6, 2018)

* Add `sleep` function for easy interval delays (#347).
* Provide `clock::now()`, a configurable source of time (#381).

# 0.2.3 (May 2, 2018)

* Improve parking semantics (#327).

# 0.2.2 (Skipped due to failure in counting module)

# 0.2.1 (April 2, 2018)

* Fix build on 32-bit systems (#274).

# 0.2.0 (March 30, 2018)

* Rewrite from scratch using a hierarchical wheel strategy (#249).

# 0.1.2 (Jun 27, 2017)

* Allow naming timer thread.
* Track changes in dependencies.

# 0.1.1 (Apr 6, 2017)

* Set Rust v1.14 as the minimum supported version.
* Fix bug related to intervals.
* Impl `PartialEq + Eq` for TimerError.
* Add `Debug` implementations.

# 0.1.0 (Jan 11, 2017)

* Initial Release
