# 0.2.0-alpha.6 (September 30, 2019)

- Move to `futures-*-preview 0.3.0-alpha.19`
- Move to `pin-project 0.4`

# 0.2.0-alpha.5 (September 19, 2019)

### Fix
- shutdown blocking pool threads when idle (#1562, #1514).

# 0.2.0-alpha.4 (August 29, 2019)

- Track tokio release.

# 0.2.0-alpha.3 (August 28, 2019)

### Changed
- use `tracing` instead of `log`

### Added
- thread pool dedicated to blocking operations (#1495).
- `Executor::spawn_with_handle` (#1492).

# 0.2.0-alpha.2 (August 17, 2019)

### Fixed
- allow running executor from within blocking clause (#1433).

### Changed
- Update `futures` dependency to 0.3.0-alpha.18.

### Added
- Import `current-thread` executor (#1447).
- Import `threadpool` executor (#1152).

# 0.2.0-alpha.1 (August 8, 2019)

### Changed
- Switch to `async`, `await`, and `std::future`.

### Removed
- `Enter::make_permanent` and `Enter::on_exit` (#???)

# 0.1.7 (March 22, 2019)

### Added
- `TypedExecutor` for spawning futures of a specific type (#993).

# 0.1.6 (January 6, 2019)

* Implement `Unpark` for `Arc<Unpark>` (#802).
* Switch to crossbeam's Parker / Unparker (#528).

# 0.1.5 (September 26, 2018)

* Implement `futures::Executor` for `DefaultExecutor` (#563).
* Add `Enter::block_on(future)` (#646)

# 0.1.4 (August 23, 2018)

* Implement `std::error::Error` for error types (#511).

# 0.1.3 (August 6, 2018)

* Implement `Executor` for `Box<E: Executor>` (#420).
* Improve `EnterError` debug message (#410).
* Implement `status`, `Send`, and `Sync` for `DefaultExecutor` (#463, #472).
* Fix race in `ParkThread` (#507).
* Handle recursive calls into `DefaultExecutor` (#473).

# 0.1.2 (March 30, 2018)

* Implement `Unpark` for `Box<Unpark>`.

# 0.1.1 (March 22, 2018)

* Optionally support futures 0.2.

# 0.1.0 (March 09, 2018)

* Initial release
