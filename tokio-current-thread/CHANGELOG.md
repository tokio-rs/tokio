# 0.1.7 (February 4, 2020)
 
* Add `tokio 0.2.x` deprecation notice.

# 0.1.6 (March 22, 2019)

### Added
- implement `TypedExecutor` (#993).

# 0.1.5 (March 1, 2019)

### Fixed
- Documentation typos (#882).

# 0.1.4 (November 21, 2018)

* Fix shutdown on idle (#763).

# 0.1.3 (September 27, 2018)

* Fix minimal versions

# 0.1.2 (September 26, 2018)

* Implement `futures::Executor` for executor types (#563)
* Spawning performance improvements (#565)

# 0.1.1 (August 6, 2018)

* Implement `std::Error` for misc error types (#501)
* bugfix: Track tasks pending in spawn queue (#478)

# 0.1.0 (June 13, 2018)

* Extract `tokio::executor::current_thread` to a tokio-current-thread crate (#356)
