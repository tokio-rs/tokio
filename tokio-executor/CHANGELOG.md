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
