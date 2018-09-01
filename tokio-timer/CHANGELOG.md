# 0.2.6 (August 23, 2018)

* Implement `Default` for `timer::Handle` (#553)
* Provide `DelayQueue` utility (#550)
* Reduce size of `Delay` struct (#554)
* Introduce `Timeout`, deprecate `Deadline` (#558)

# 0.2.5 (August 6, 2018)

* Add `Interval::interval` shortcut (#492).

# 0.2.4 (June 6, 2018)

* Add `sleep` function for easy interval delays (#347).
* Provide `clock::now()`, a configurable source of time (#381).

# 0.2.3 (May 2, 2018)

* Improve parking semantics (#327).

# 0.2.2 (Skipped due to failure in counting module)

# 0.2.1 (April 2, 2018)

* Fix build on 32-bit systems (#274).

# 0.2.0 (March 30, 2018)

* Rewrite from scratch using a hierarchical wheel strategy (#249).

# 0.1.2 (Jun 27, 2017)

* Allow naming timer thread.
* Track changes in dependencies.

# 0.1.1 (Apr 6, 2017)

* Set Rust v1.14 as the minimum supported version.
* Fix bug related to intervals.
* Impl `PartialEq + Eq` for TimerError.
* Add `Debug` implementations.

# 0.1.0 (Jan 11, 2017)

* Initial Release
