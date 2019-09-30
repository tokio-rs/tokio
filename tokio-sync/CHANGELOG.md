# 0.2.0-alpha.6 (September 30, 2019)

- Move to `futures-*-preview 0.3.0-alpha.19`
- Move to `pin-project 0.4`

# 0.2.0-alpha.5 (September 19, 2019)

### Changed
- rename `Lock` -> `Mutex` and make it more like `std::sync::Mutex` (#1573).

### Added
- `Barrier`, an async version of `std::sync::Barrier` (#1571).

# 0.2.0-alpha.4 (August 29, 2019)

- Track tokio release.

# 0.2.0-alpha.3 (August 23, 2019)

- Track `tokio` version number

# 0.2.0-alpha.2 (August 17, 2019)

### Changed
- Update `futures` dependency to 0.3.0-alpha.18.

# 0.2.0-alpha.1 (August 8, 2019)

### Changed
- Switch to `async`, `await`, and `std::future`.

# 0.1.6 (June 4, 2019)

### Added
- Add Sync impl for Lock (#1117).

# 0.1.5 (April 22, 2019)

### Added
- Add asynchronous mutual exclusion primitive (#964).

# 0.1.4 (March 13, 2019)

### Fixed
- Fix memory leak on channel drop (#917).

### Added
- `std::error::Error` implementation for `oneshot`, `watch` error types (#967).

# 0.1.3 (March 1, 2019)

### Added
- `Watch`, a single value broadcast channel (#922).
- `std::error::Error` implementation for more `mpsc` types (#937).

# 0.1.2 (February 20, 2019)

### Fixes
- `mpsc` and `Semaphore` when releasing permits (#904).
- `oneshot` task handle leak (#911).

### Changes
- Performance improvements in `AtomicTask` (#892).
- Improved assert message when creating a channel with bound of zero (#906).

### Adds
- `AtomicTask::take_task` (#895).

# 0.1.1 (February 1, 2019)

### Fixes
- Panic when creating a channel with bound 0 (#879).

# 0.1.0 (January 24, 2019)

- Initial Release
