# 0.1.8 (February 4, 2020)

* Add `tokio 0.2.x` deprecation notice.

# 0.1.7 (October 10, 2019)

### Fixed
- memory leak when polling oneshot handle from more than one task (#1649).

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
