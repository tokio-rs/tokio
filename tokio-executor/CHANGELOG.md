# 0.1.10 (February 4, 2020)

* Add `tokio 0.2.x` deprecation notice.

# 0.1.9 (November 27, 2019)

### Added
- Add `executor::set_default` which behaves like `with_default` but returns a
  drop guard (#1725).

# 0.1.8 (June 2, 2019)

### Added
- Add `executor::exit` to allow other executors inside `threadpool::blocking` (#1155).

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
