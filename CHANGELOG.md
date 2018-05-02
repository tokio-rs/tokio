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
