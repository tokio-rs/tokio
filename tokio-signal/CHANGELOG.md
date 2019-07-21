# 0.3.0

### Changed
- **Breaking:** `windows::Event` has been removed in favor of `CtrlC` and
a separate `windows::CtrlBreak` struct.
- **Breaking:** `ctrl_c{,_with_handle}` has been replaced with a `CtrlC` struct
(which can be constructed via `CtrlC::{new, with_handle}`.
- **Breaking:** `unix::Signal` returns `()` instead of the signal number used in registration
- **Breaking:** `unis::Signal` constructors now return a simple result rather than a lazy future

# 0.2.9

### Fixed
- `windows::Event` performs internal registrations lazily, so now it can be
constructed outside of a running task
- remove usage of deprecated `Handle::current` in default `windows::Event`
constructors

# 0.2.8 (March 22, 2019)

### Fixed
- remove usage of deprecated `Handle::current` (#981).

## 0.2.7 - (November 21, 2018)
### Changed
* `unix::Signal` now implements `Sync`
* minimize allocations

### Fixes
* `unix::Signal` now avoids extraneous wakeups generated as a result of
dropping other instances

## 0.2.6 - (October 26, 2018)
### Changed
* Use the `signal-hook` crate for managing signal registrations

## 0.2.5 - (September 29, 2018)
### Fixes
* Fix a possible starvation when polling multiple `Signal` instances outside of
a tokio reactor (e.g. by using `Future::wait`)

## 0.2.4 - (August 25, 2018)
### Fixes
* Actually make `unix::bsd` public

## 0.2.3 - (August 25, 2018)
### Features
* Exposes `SIGINFO` on BSD-based operating systems.

## 0.2.2 - (August 14, 2018)
### Fixes
* Fix starvation of `Signal`s whenever a `Signal` instance is dropped
* Fix starvation of individual `Signal`s based on their creation order

## 0.2.1 - (May 27, 2018)
### Fixes
* Bump minimum supported version of `mio` to 0.6.14

## 0.2.0 - (May 7, 2018)
#### Features
 * Uses `tokio` instead of `tokio_core`
 * Supports all 33 signals on FreeBSD
