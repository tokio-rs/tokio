# 0.1.6 (August 23, 2018)

* Misc performance improvements (#466, #468, #470, #475, #534)
* Documentation improvements (#450)
* Shutdown backup threads when idle (#489)
* Implement std::error::Error for error types (#511)
* Bugfix: handle num_cpus returning zero (#530).

# 0.1.5 (July 3, 2018)

* Fix race condition bug when threads are woken up (#459).
* Improve `BlockingError` message (#451).

# 0.1.4 (June 6, 2018)

* Fix bug that can occur with multiple pools in a process (#375).

# 0.1.3 (May 2, 2018)

* Add `blocking` annotation (#317).

# 0.1.2 (March 30, 2018)

* Add the ability to specify a custom thread parker.

# 0.1.1 (March 22, 2018)

* Handle futures that panic on the threadpool.
* Optionally support futures 0.2.

# 0.1.0 (March 09, 2018)

* Initial release
