# 0.1.11 (January 24, 2019)

### Fixed
- Drop incomplete tasks when threadpool is dropped (#722).

# 0.1.10 (January 6, 2019)

* Fix deadlock bug in `blocking` (#795).
* Introduce global task queue (#798).
* Use crossbeam's Parker / Unparker (#529).
* Panic if worker thread cannot be spawned (#826).
* Improve `blocking` API documentation (#789).

# 0.1.9 (November 21, 2018)

* Bump internal dependency versions (#746, #753).
* Internal refactors (#768, #769).

# 0.1.8 (October 23, 2018)

* Assign spawned tasks to random worker (#660).
* Worker threads no longer shutdown (#692).
* Reduce atomic ops in notifier (#702).

# 0.1.7 (September 27, 2018)

* Add ThreadPool::spawn_handle (#602, #604).
* Fix spawned future leak (#649).

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
