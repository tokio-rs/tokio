## 0.2.5 - 2020-02-04

* Add `tokio 0.2.x` deprecation notice.

## 0.2.4 - 2019-06-21
### Fixed
* Proccesses "leaked" via `Child::forget` now reaped rather than left as zombies
for the duration of the parent process.
* Dropping a `Child` process no longer blocks the caller until the process fully
exits. This avoids a pathological deadlock if the kernel doesn't kill the child.

### Changed
* Updated the example program for reading lines from a child process to be more
flexible to be copy/pasted and iterated upon.

## 0.2.3 - 2018-11-01
### Added
* `ChildStd{in, out, err}` now implement `AsRawFd`/`AsRawHandle` on Unix/Windows
systems, respectively.

## 0.2.2 - 2018-05-27
### Fixed
- Fixed a pathological situation where a signal could be missed if it arrived
after polling the child but before registering for a new notification

## 0.2.1 - 2018-05-18
### Changed
- **Breaking**: asynchronous spawning of a child process now requires using a
reactor handle from the `tokio` crate instead of the `tokio-core` crate
- Child processes may be spawned without specifying a `tokio` handle at all
(the current/default reactor handle will be used)
### Removed
- **Breaking**: removed all previously deprecated items

## 0.1.6 - 2018-05-09
### Fixed
- On Unix systems, any child processes that are `kill`ed (or implicitly killed
via dropping the child without calling `forget`) are no longer left in a zombie
state, which allows the OS to reclaim the process.

## 0.1.5 - 2018-01-03
### Changed
- Minimum required version of `winapi` has been bumped to `0.3`.

## 0.1.4 - 2017-06-25
### Fixed
- Added missing `Debug` impls on all types.
- Added missing `must_use` annotations on all futures.
- Ensure `status_async` closes child's stdio handles after spawning in order
to prevent potential deadlocks when attempting to interact with any pipes held
by the parent process.

## 0.1.3 - 2017-03-15
### Changed
- Minimum required version of `futures` has been bumped to `0.1.11`.
- Minimum required version of `mio` has been bumped to `0.6.5`.
- Minimum required version of `tokio-core` has been bumped to `0.1.6`.

## 0.1.2 - 2017-01-24
### Changed
- Minimum required version of `tokio-signal` has been bumped to `0.1.2`.
### Fixed
- The event loop which spawns the first async child no longer needs to be kept
alive for subsequent child spawns to make progress.

## 0.1.1 - 2016-12-19
### Added
- Support performing async I/O operations on the child's stdio handles.
### Changed
- Functionality has been reimplemented as the `CommandExt` extension trait
(implemented directly on `std::process::Command`) instead of going through
the locally vendored `Command` type.

## 0.1.0 - 2016-09-10
- First release!
