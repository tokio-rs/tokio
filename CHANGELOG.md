This changelog only applies to the `tokio` crate proper. Each sub crate
maintains its own changelog tracking changes made in each respective sub crate.

# 0.1.11 (September 28, 2018)

* Fix `tokio-async-await` dependency (#675).

# 0.1.10 (September 27, 2018)

* Fix minimal versions

# 0.1.9 (September 27, 2018)

* Experimental async/await improvements (#661).
* Re-export `TaskExecutor` from `tokio-current-thread` (#652).
* Improve `Runtime` builder API (#645).
* `tokio::run` panics when called from the context of an executor
  (#646).
* Introduce `StreamExt` with a `timeout` helper (#573).
* Move `length_delimited` into `tokio` (#575).
* Re-organize `tokio::net` module (#548).
* Re-export `tokio-current-thread::spawn` in current_thread runtime
  (#579).

# 0.1.8 (August 23, 2018)

* Extract tokio::executor::current_thread to a sub crate (#370)
* Add `Runtime::block_on` (#398)
* Add `runtime::current_thread::block_on_all` (#477)
* Misc documentation improvements (#450)
* Implement `std::error::Error` for error types (#501)

# 0.1.7 (June 6, 2018)

* Add `Runtime::block_on` for concurrent runtime (#391).
* Provide handle to `current_thread::Runtime` that allows spawning tasks from
  other threads (#340).
* Provide `clock::now()`, a configurable source of time (#381).

# 0.1.6 (May 2, 2018)

* Add asynchronous filesystem APIs (#323).
* Add "current thread" runtime variant (#308).
* `CurrentThread`: Expose inner `Park` instance.
* Improve fairness of `CurrentThread` executor (#313).

# 0.1.5 (March 30, 2018)

* Provide timer API (#266)

# 0.1.4 (March 22, 2018)

* Fix build on FreeBSD (#218)
* Shutdown the Runtime when the handle is dropped (#214)
* Set Runtime thread name prefix for worker threads (#232)
* Add builder for Runtime (#234)
* Extract TCP and UDP types into separate crates (#224)
* Optionally support futures 0.2.

# 0.1.3 (March 09, 2018)

* Fix `CurrentThread::turn` to block on idle (#212).

# 0.1.2 (March 09, 2018)

* Introduce Tokio Runtime (#141)
* Provide `CurrentThread` for more flexible usage of current thread executor (#141).
* Add Lio for platforms that support it (#142).
* I/O resources now lazily bind to the reactor (#160).
* Extract Reactor to dedicated crate (#169)
* Add facade to sub crates and add prelude (#166).
* Switch TCP/UDP fns to poll_ -> Poll<...> style (#175)

# 0.1.1 (February 09, 2018)

* Doc fixes

# 0.1.0 (February 07, 2018)

* Initial crate released based on [RFC](https://github.com/tokio-rs/tokio-rfcs/pull/3).
