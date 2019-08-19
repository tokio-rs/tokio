# 0.2.0-alpha.2 (August 17, 2019)

### Changed
- Renamed `tokio-net` from `tokio-reactor` (#1450).
- Switch `with_default(..., || )` to `set_default(...) -> Guard` (#1449).
- Update `futures` dependency to 0.3.0-alpha.18.

### Added
- Import `tokio-tcp` (#1456).
- Import `tokio-udp` (#1459).
- Import `tokio-uds` (#1462).
- Import `tokio-signal` (#1463).

# 0.2.0-alpha.1 (August 8, 2019)

### Changed
- Switch to `async`, `await`, and `std::future`.

# 0.1.9 (March 1, 2019)

### Added
- impl `AsRawFd` for `Reactor` on unix platforms (#890).

### Changed
- perf: reduce unnecessary task clones (#899).
- perf: release lock before issuing syscall (#894).

# 0.1.8 (January 6, 2019)

* Update to `parking_lot` 0.7 (#778).
* Deprecate `Handle::current()` (#805).

# 0.1.7 (November 21, 2018)

* Reduce log level to trace (#734).
* Bump internal dependency versions (#746).

# 0.1.6 (September 27, 2018)

* Fix panic when reactor is stored in a thread-local (#628).

# 0.1.5 (August 27, 2018)

* Experimental async / await support.

# 0.1.4 (August 23, 2018)

* Use a scalable RW lock (#517)
* Implement std::error::Error for error types (#511)
* Documentation improvements

# 0.1.3 (August 6, 2018)

* Misc small fixes (#508)

# 0.1.2 (June 13, 2018)

* Fix deadlock that can happen when shutting down (#409)
* Handle::default() lazily binds to reactor (#350)

# 0.1.1 (March 22, 2018)

* Fix threading bugs (#227)
* Fix notification bugs (#243)
* Optionally support futures 0.2 (#172)

# 0.1.0 (March 09, 2018)

* Initial release
