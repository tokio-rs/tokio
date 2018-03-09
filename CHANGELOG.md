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
