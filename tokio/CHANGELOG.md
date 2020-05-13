# 0.2.21 (May 13, 2020)

### Fixes

- macros: disambiguate built-in `#[test]` attribute in macro expansion (#2503)
- rt: `LocalSet` and task budgeting (#2462).
- rt: task budgeting with `block_in_place` (#2502).
- sync: release `broadcast` channel memory without sending a value (#2509).
- time: notify when resetting a `Delay` to a time in the past (#2290).

### Added
- io: `get_mut`, `get_ref`, and `into_inner` to `Lines` (#2450).
- io: `mio::Ready` argument to `PollEvented` (#2419).
- os: illumos support (#2486).
- rt: `Handle::spawn_blocking` (#2501).
- sync: `OwnedMutexGuard` for `Arc<Mutex<T>>` (#2455).

# 0.2.20 (April 28, 2020)

### Fixes
- sync: `broadcast` closing the channel no longer requires capacity (#2448).
- rt: regression when configuring runtime with `max_threads` less than number of CPUs (#2457).

# 0.2.19 (April 24, 2020)

### Fixes
- docs: misc improvements (#2400, #2405, #2414, #2420, #2423, #2426, #2427, #2434, #2436, #2440).
- rt: support `block_in_place` in more contexts (#2409, #2410).
- stream: no panic in `merge()` and `chain()` when using `size_hint()` (#2430).
- task: include visibility modifier when defining a task-local (#2416).

### Added
- rt: `runtime::Handle::block_on` (#2437).
- sync: owned `Semaphore` permit (#2421).
- tcp: owned split (#2270).

# 0.2.18 (April 12, 2020)

### Fixes
- task: `LocalSet` was incorrectly marked as `Send` (#2398)
- io: correctly report `WriteZero` failure in `write_int` (#2334)

# 0.2.17 (April 9, 2020)

### Fixes
- rt: bug in work-stealing queue (#2387)

### Changes
- rt: threadpool uses logical CPU count instead of physical by default (#2391)

# 0.2.16 (April 3, 2020)

### Fixes

- sync: fix a regression where `Mutex`, `Semaphore`, and `RwLock` futures no
  longer implement `Sync` ([#2375])
- fs: fix `fs::copy` not copying file permissions ([#2354])

### Added

- time: added `deadline` method to `delay_queue::Expired` ([#2300])
- io: added `StreamReader` ([#2052])

# 0.2.15 (April 2, 2020)

### Fixes

- rt: fix queue regression ([#2362]).

### Added

- sync: Add disarm to `mpsc::Sender` ([#2358]).

# 0.2.14 (April 1, 2020)

### Fixes
- rt: concurrency bug in scheduler ([#2273]).
- rt: concurrency bug with shell runtime ([#2333]).
- test-util: correct pause/resume of time ([#2253]).
- time: `DelayQueue` correct wakeup after `insert` ([#2285]).

### Added
- io: impl `RawFd`, `AsRawHandle` for std io types ([#2335]).
- rt: automatic cooperative task yielding (#2160, #2343, #2349).
- sync: `RwLock::into_inner` ([#2321]).

### Changed
- sync: semaphore, mutex internals rewritten to avoid allocations ([#2325]).

# 0.2.13 (February 28, 2020)

### Fixes
- macros: unresolved import in `pin!` ([#2281]).

# 0.2.12 (February 27, 2020)

### Fixes
- net: `UnixStream::poll_shutdown` should call `shutdown(Write)` ([#2245]).
- process: Wake up read and write on `EPOLLERR` ([#2218]).
- rt: potential deadlock when using `block_in_place` and shutting down the
  runtime ([#2119]).
- rt: only detect number of CPUs if `core_threads` not specified ([#2238]).
- sync: reduce `watch::Receiver` struct size ([#2191]).
- time: succeed when setting delay of `$MAX-1` ([#2184]).
- time: avoid having to poll `DelayQueue` after inserting new delay ([#2217]).

### Added
- macros: `pin!` variant that assigns to identifier and pins ([#2274]).
- net: impl `Stream` for `Listener` types ([#2275]).
- rt: `Runtime::shutdown_timeout` waits for runtime to shutdown for specified
  duration ([#2186]).
- stream: `StreamMap` merges streams and can insert / remove streams at
  runtime ([#2185]).
- stream: `StreamExt::skip()` skips a fixed number of items ([#2204]).
- stream: `StreamExt::skip_while()` skips items based on a predicate ([#2205]).
- sync: `Notify` provides basic `async` / `await` task notification ([#2210]).
- sync: `Mutex::into_inner` retrieves guarded data ([#2250]).
- sync: `mpsc::Sender::send_timeout` sends, waiting for up to specified duration
  for channel capacity ([#2227]).
- time: impl `Ord` and `Hash` for `Instant` ([#2239]).

# 0.2.11 (January 27, 2020)

### Fixes
- docs: misc fixes and tweaks (#2155, #2103, #2027, #2167, #2175).
- macros: handle generics in `#[tokio::main]` method ([#2177]).
- sync: `broadcast` potential lost notifications ([#2135]).
- rt: improve "no runtime" panic messages ([#2145]).

### Added
- optional support for using `parking_lot` internally ([#2164]).
- fs: `fs::copy`, an async version of `std::fs::copy` ([#2079]).
- macros: `select!` waits for the first branch to complete ([#2152]).
- macros: `join!` waits for all branches to complete ([#2158]).
- macros: `try_join!` waits for all branches to complete or the first error ([#2169]).
- macros: `pin!` pins a value to the stack ([#2163]).
- net: `ReadHalf::poll()` and `ReadHalf::poll_peak` ([#2151])
- stream: `StreamExt::timeout()` sets a per-item max duration ([#2149]).
- stream: `StreamExt::fold()` applies a function, producing a single value. ([#2122]).
- sync: impl `Eq`, `PartialEq` for `oneshot::RecvError` ([#2168]).
- task: methods for inspecting the `JoinError` cause ([#2051]).

# 0.2.10 (January 21, 2020)

### Fixes
- `#[tokio::main]` when `rt-core` feature flag is not enabled ([#2139]).
- remove `AsyncBufRead` from `BufStream` impl block ([#2108]).
- potential undefined behavior when implementing `AsyncRead` incorrectly ([#2030]).

### Added
- `BufStream::with_capacity` ([#2125]).
- impl `From` and `Default` for `RwLock` ([#2089]).
- `io::ReadHalf::is_pair_of` checks if provided `WriteHalf` is for the same
  underlying object (#1762, #2144).
- `runtime::Handle::try_current()` returns a handle to the current runtime ([#2118]).
- `stream::empty()` returns an immediately ready empty stream ([#2092]).
- `stream::once(val)` returns a stream that yields a single value: `val` ([#2094]).
- `stream::pending()` returns a stream that never becomes ready ([#2092]).
- `StreamExt::chain()` sequences a second stream after the first completes ([#2093]).
- `StreamExt::collect()` transform a stream into a collection ([#2109]).
- `StreamExt::fuse` ends the stream after the first `None` ([#2085]).
- `StreamExt::merge` combines two streams, yielding values as they become ready ([#2091]).
- Task-local storage ([#2126]).

# 0.2.9 (January 9, 2020)

### Fixes
- `AsyncSeek` impl for `File` ([#1986]).
- rt: shutdown deadlock in `threaded_scheduler` (#2074, #2082).
- rt: memory ordering when dropping `JoinHandle` ([#2044]).
- docs: misc API documentation fixes and improvements.

# 0.2.8 (January 7, 2020)

### Fixes
- depend on new version of `tokio-macros`.

# 0.2.7 (January 7, 2020)

### Fixes
- potential deadlock when dropping `basic_scheduler` Runtime.
- calling `spawn_blocking` from within a `spawn_blocking` ([#2006]).
- storing a `Runtime` instance in a thread-local ([#2011]).
- miscellaneous documentation fixes.
- rt: fix `Waker::will_wake` to return true when tasks match ([#2045]).
- test-util: `time::advance` runs pending tasks before changing the time ([#2059]).

### Added
- `net::lookup_host` maps a `T: ToSocketAddrs` to a stream of `SocketAddrs` ([#1870]).
- `process::Child` fields are made public to match `std` ([#2014]).
- impl `Stream` for `sync::broadcast::Receiver` ([#2012]).
- `sync::RwLock` provides an asynchonous read-write lock ([#1699]).
- `runtime::Handle::current` returns the handle for the current runtime ([#2040]).
- `StreamExt::filter` filters stream values according to a predicate ([#2001]).
- `StreamExt::filter_map` simultaneously filter and map stream values ([#2001]).
- `StreamExt::try_next` convenience for streams of `Result<T, E>` ([#2005]).
- `StreamExt::take` limits a stream to a specified number of values ([#2025]).
- `StreamExt::take_while` limits a stream based on a predicate ([#2029]).
- `StreamExt::all` tests if every element of the stream matches a predicate ([#2035]).
- `StreamExt::any` tests if any element of the stream matches a predicate ([#2034]).
- `task::LocalSet.await` runs spawned tasks until the set is idle ([#1971]).
- `time::DelayQueue::len` returns the number entries in the queue ([#1755]).
- expose runtime options from the `#[tokio::main]` and `#[tokio::test]` ([#2022]).

# 0.2.6 (December 19, 2019)

### Fixes
- `fs::File::seek` API regression ([#1991]).

# 0.2.5 (December 18, 2019)

### Added
- `io::AsyncSeek` trait ([#1924]).
- `Mutex::try_lock` ([#1939])
- `mpsc::Receiver::try_recv` and `mpsc::UnboundedReceiver::try_recv` ([#1939]).
- `writev` support for `TcpStream` ([#1956]).
- `time::throttle` for throttling streams ([#1949]).
- implement `Stream` for `time::DelayQueue` ([#1975]).
- `sync::broadcast` provides a fan-out channel ([#1943]).
- `sync::Semaphore` provides an async semaphore ([#1973]).
- `stream::StreamExt` provides stream utilities ([#1962]).

### Fixes
- deadlock risk while shutting down the runtime ([#1972]).
- panic while shutting down the runtime ([#1978]).
- `sync::MutexGuard` debug output ([#1961]).
- misc doc improvements (#1933, #1934, #1940, #1942).

### Changes
- runtime threads are configured with `runtime::Builder::core_threads` and
  `runtime::Builder::max_threads`. `runtime::Builder::num_threads` is
  deprecated ([#1977]).

# 0.2.4 (December 6, 2019)

### Fixes
- `sync::Mutex` deadlock when `lock()` future is dropped early ([#1898]).

# 0.2.3 (December 6, 2019)

### Added
- read / write integers using `AsyncReadExt` and `AsyncWriteExt` ([#1863]).
- `read_buf` / `write_buf` for reading / writing `Buf` / `BufMut` ([#1881]).
- `TcpStream::poll_peek` - pollable API for performing TCP peek ([#1864]).
- `sync::oneshot::error::TryRecvError` provides variants to detect the error
  kind ([#1874]).
- `LocalSet::block_on` accepts `!'static` task ([#1882]).
- `task::JoinError` is now `Sync` ([#1888]).
- impl conversions between `tokio::time::Instant` and
  `std::time::Instant` ([#1904]).

### Fixes
- calling `spawn_blocking` after runtime shutdown ([#1875]).
- `LocalSet` drop inifinite loop ([#1892]).
- `LocalSet` hang under load ([#1905]).
- improved documentation (#1865, #1866, #1868, #1874, #1876, #1911).

# 0.2.2 (November 29, 2019)

### Fixes
- scheduling with `basic_scheduler` ([#1861]).
- update `spawn` panic message to specify that a task scheduler is required ([#1839]).
- API docs example for `runtime::Builder` to include a task scheduler ([#1841]).
- general documentation ([#1834]).
- building on illumos/solaris ([#1772]).
- panic when dropping `LocalSet` ([#1843]).
- API docs mention the required Cargo features for `Builder::{basic, threaded}_scheduler` ([#1858]).

### Added
- impl `Stream` for `signal::unix::Signal` ([#1849]).
- API docs for platform specific behavior of `signal::ctrl_c` and `signal::unix::Signal` ([#1854]).
- API docs for `signal::unix::Signal::{recv, poll_recv}` and `signal::windows::CtrlBreak::{recv, poll_recv}` ([#1854]).
- `File::into_std` and `File::try_into_std` methods ([#1856]).

# 0.2.1 (November 26, 2019)

### Fixes
- API docs for `TcpListener::incoming`, `UnixListener::incoming` ([#1831]).

### Added
- `tokio::task::LocalSet` provides a strategy for spawning `!Send` tasks ([#1733]).
- export `tokio::time::Elapsed` ([#1826]).
- impl `AsRawFd`, `AsRawHandle` for `tokio::fs::File` ([#1827]).

# 0.2.0 (November 26, 2019)

A major breaking change. Most implementation and APIs have changed one way or
another. This changelog entry contains a highlight

### Changed
- APIs are updated to use `async / await`.
- most `tokio-*` crates are collapsed into this crate.
- Scheduler is rewritten.
- `tokio::spawn` returns a `JoinHandle`.
- A single I/O / timer is used per runtime.
- I/O driver uses a concurrent slab for allocating state.
- components are made available via feature flag.
- Use `bytes` 0.5
- `tokio::codec` is moved to `tokio-util`.

### Removed
- Standalone `timer` and `net` drivers are removed, use `Runtime` instead
- `current_thread` runtime is removed, use `tokio::runtime::Runtime` with
  `basic_scheduler` instead.

# 0.1.21 (May 30, 2019)

### Changed
- Bump `tokio-trace-core` version to 0.2 ([#1111]).

# 0.1.20 (May 14, 2019)

### Added
- `tokio::runtime::Builder::panic_handler` allows configuring handling
  panics on the runtime ([#1055]).

# 0.1.19 (April 22, 2019)

### Added
- Re-export `tokio::sync::Mutex` primitive ([#964]).

# 0.1.18 (March 22, 2019)

### Added
- `TypedExecutor` re-export and implementations ([#993]).

# 0.1.17 (March 13, 2019)

### Added
- Propagate trace subscriber in the runtime ([#966]).

# 0.1.16 (March 1, 2019)

### Fixed
- async-await: track latest nightly changes ([#940]).

### Added
- `sync::Watch`, a single value broadcast channel ([#922]).
- Async equivalent of read / write file helpers being added to `std` ([#896]).

# 0.1.15 (January 24, 2019)

### Added
- Re-export tokio-sync APIs ([#839]).
- Stream enumerate combinator ([#832]).

# 0.1.14 (January 6, 2019)

* Use feature flags to break up the crate, allowing users to pick & choose
  components ([#808]).
* Export `UnixDatagram` and `UnixDatagramFramed` ([#772]).

# 0.1.13 (November 21, 2018)

* Fix `Runtime::reactor()` when no tasks are spawned ([#721]).
* `runtime::Builder` no longer uses deprecated methods ([#749]).
* Provide `after_start` and `before_stop` configuration settings for
  `Runtime` ([#756]).
* Implement throttle stream combinator ([#736]).

# 0.1.12 (October 23, 2018)

* runtime: expose `keep_alive` on runtime builder ([#676]).
* runtime: create a reactor per worker thread ([#660]).
* codec: fix panic in `LengthDelimitedCodec` ([#682]).
* io: re-export `tokio_io::io::read` function ([#689]).
* runtime: check for executor re-entry in more places ([#708]).

# 0.1.11 (September 28, 2018)

* Fix `tokio-async-await` dependency ([#675]).

# 0.1.10 (September 27, 2018)

* Fix minimal versions

# 0.1.9 (September 27, 2018)

* Experimental async/await improvements ([#661]).
* Re-export `TaskExecutor` from `tokio-current-thread` ([#652]).
* Improve `Runtime` builder API ([#645]).
* `tokio::run` panics when called from the context of an executor
  ([#646]).
* Introduce `StreamExt` with a `timeout` helper ([#573]).
* Move `length_delimited` into `tokio` ([#575]).
* Re-organize `tokio::net` module ([#548]).
* Re-export `tokio-current-thread::spawn` in current_thread runtime
  ([#579]).

# 0.1.8 (August 23, 2018)

* Extract tokio::executor::current_thread to a sub crate ([#370])
* Add `Runtime::block_on` ([#398])
* Add `runtime::current_thread::block_on_all` ([#477])
* Misc documentation improvements ([#450])
* Implement `std::error::Error` for error types ([#501])

# 0.1.7 (June 6, 2018)

* Add `Runtime::block_on` for concurrent runtime ([#391]).
* Provide handle to `current_thread::Runtime` that allows spawning tasks from
  other threads ([#340]).
* Provide `clock::now()`, a configurable source of time ([#381]).

# 0.1.6 (May 2, 2018)

* Add asynchronous filesystem APIs ([#323]).
* Add "current thread" runtime variant ([#308]).
* `CurrentThread`: Expose inner `Park` instance.
* Improve fairness of `CurrentThread` executor ([#313]).

# 0.1.5 (March 30, 2018)

* Provide timer API ([#266])

# 0.1.4 (March 22, 2018)

* Fix build on FreeBSD ([#218])
* Shutdown the Runtime when the handle is dropped ([#214])
* Set Runtime thread name prefix for worker threads ([#232])
* Add builder for Runtime ([#234])
* Extract TCP and UDP types into separate crates ([#224])
* Optionally support futures 0.2.

# 0.1.3 (March 09, 2018)

* Fix `CurrentThread::turn` to block on idle ([#212]).

# 0.1.2 (March 09, 2018)

* Introduce Tokio Runtime ([#141])
* Provide `CurrentThread` for more flexible usage of current thread executor ([#141]).
* Add Lio for platforms that support it ([#142]).
* I/O resources now lazily bind to the reactor ([#160]).
* Extract Reactor to dedicated crate ([#169])
* Add facade to sub crates and add prelude ([#166]).
* Switch TCP/UDP fns to poll_ -> Poll<...> style ([#175])

# 0.1.1 (February 09, 2018)

* Doc fixes

# 0.1.0 (February 07, 2018)

* Initial crate released based on [RFC](https://github.com/tokio-rs/tokio-rfcs/pull/3).

[#2375]: https://github.com/tokio-rs/tokio/pull/2375
[#2362]: https://github.com/tokio-rs/tokio/pull/2362
[#2358]: https://github.com/tokio-rs/tokio/pull/2358
[#2354]: https://github.com/tokio-rs/tokio/pull/2354
[#2335]: https://github.com/tokio-rs/tokio/pull/2335
[#2333]: https://github.com/tokio-rs/tokio/pull/2333
[#2325]: https://github.com/tokio-rs/tokio/pull/2325
[#2321]: https://github.com/tokio-rs/tokio/pull/2321
[#2300]: https://github.com/tokio-rs/tokio/pull/2300
[#2285]: https://github.com/tokio-rs/tokio/pull/2285
[#2281]: https://github.com/tokio-rs/tokio/pull/2281
[#2275]: https://github.com/tokio-rs/tokio/pull/2275
[#2274]: https://github.com/tokio-rs/tokio/pull/2274
[#2273]: https://github.com/tokio-rs/tokio/pull/2273
[#2253]: https://github.com/tokio-rs/tokio/pull/2253
[#2250]: https://github.com/tokio-rs/tokio/pull/2250
[#2245]: https://github.com/tokio-rs/tokio/pull/2245
[#2239]: https://github.com/tokio-rs/tokio/pull/2239
[#2238]: https://github.com/tokio-rs/tokio/pull/2238
[#2227]: https://github.com/tokio-rs/tokio/pull/2227
[#2218]: https://github.com/tokio-rs/tokio/pull/2218
[#2217]: https://github.com/tokio-rs/tokio/pull/2217
[#2210]: https://github.com/tokio-rs/tokio/pull/2210
[#2205]: https://github.com/tokio-rs/tokio/pull/2205
[#2204]: https://github.com/tokio-rs/tokio/pull/2204
[#2191]: https://github.com/tokio-rs/tokio/pull/2191
[#2186]: https://github.com/tokio-rs/tokio/pull/2186
[#2185]: https://github.com/tokio-rs/tokio/pull/2185
[#2184]: https://github.com/tokio-rs/tokio/pull/2184
[#2177]: https://github.com/tokio-rs/tokio/pull/2177
[#2169]: https://github.com/tokio-rs/tokio/pull/2169
[#2168]: https://github.com/tokio-rs/tokio/pull/2168
[#2164]: https://github.com/tokio-rs/tokio/pull/2164
[#2163]: https://github.com/tokio-rs/tokio/pull/2163
[#2158]: https://github.com/tokio-rs/tokio/pull/2158
[#2152]: https://github.com/tokio-rs/tokio/pull/2152
[#2151]: https://github.com/tokio-rs/tokio/pull/2151
[#2149]: https://github.com/tokio-rs/tokio/pull/2149
[#2145]: https://github.com/tokio-rs/tokio/pull/2145
[#2139]: https://github.com/tokio-rs/tokio/pull/2139
[#2135]: https://github.com/tokio-rs/tokio/pull/2135
[#2126]: https://github.com/tokio-rs/tokio/pull/2126
[#2125]: https://github.com/tokio-rs/tokio/pull/2125
[#2122]: https://github.com/tokio-rs/tokio/pull/2122
[#2119]: https://github.com/tokio-rs/tokio/pull/2119
[#2118]: https://github.com/tokio-rs/tokio/pull/2118
[#2109]: https://github.com/tokio-rs/tokio/pull/2109
[#2108]: https://github.com/tokio-rs/tokio/pull/2108
[#2094]: https://github.com/tokio-rs/tokio/pull/2094
[#2093]: https://github.com/tokio-rs/tokio/pull/2093
[#2092]: https://github.com/tokio-rs/tokio/pull/2092
[#2091]: https://github.com/tokio-rs/tokio/pull/2091
[#2089]: https://github.com/tokio-rs/tokio/pull/2089
[#2085]: https://github.com/tokio-rs/tokio/pull/2085
[#2079]: https://github.com/tokio-rs/tokio/pull/2079
[#2059]: https://github.com/tokio-rs/tokio/pull/2059
[#2052]: https://github.com/tokio-rs/tokio/pull/2052
[#2051]: https://github.com/tokio-rs/tokio/pull/2051
[#2045]: https://github.com/tokio-rs/tokio/pull/2045
[#2044]: https://github.com/tokio-rs/tokio/pull/2044
[#2040]: https://github.com/tokio-rs/tokio/pull/2040
[#2035]: https://github.com/tokio-rs/tokio/pull/2035
[#2034]: https://github.com/tokio-rs/tokio/pull/2034
[#2030]: https://github.com/tokio-rs/tokio/pull/2030
[#2029]: https://github.com/tokio-rs/tokio/pull/2029
[#2025]: https://github.com/tokio-rs/tokio/pull/2025
[#2022]: https://github.com/tokio-rs/tokio/pull/2022
[#2014]: https://github.com/tokio-rs/tokio/pull/2014
[#2012]: https://github.com/tokio-rs/tokio/pull/2012
[#2011]: https://github.com/tokio-rs/tokio/pull/2011
[#2006]: https://github.com/tokio-rs/tokio/pull/2006
[#2005]: https://github.com/tokio-rs/tokio/pull/2005
[#2001]: https://github.com/tokio-rs/tokio/pull/2001
[#1991]: https://github.com/tokio-rs/tokio/pull/1991
[#1986]: https://github.com/tokio-rs/tokio/pull/1986
[#1978]: https://github.com/tokio-rs/tokio/pull/1978
[#1977]: https://github.com/tokio-rs/tokio/pull/1977
[#1975]: https://github.com/tokio-rs/tokio/pull/1975
[#1973]: https://github.com/tokio-rs/tokio/pull/1973
[#1972]: https://github.com/tokio-rs/tokio/pull/1972
[#1971]: https://github.com/tokio-rs/tokio/pull/1971
[#1962]: https://github.com/tokio-rs/tokio/pull/1962
[#1961]: https://github.com/tokio-rs/tokio/pull/1961
[#1956]: https://github.com/tokio-rs/tokio/pull/1956
[#1949]: https://github.com/tokio-rs/tokio/pull/1949
[#1943]: https://github.com/tokio-rs/tokio/pull/1943
[#1939]: https://github.com/tokio-rs/tokio/pull/1939
[#1924]: https://github.com/tokio-rs/tokio/pull/1924
[#1905]: https://github.com/tokio-rs/tokio/pull/1905
[#1904]: https://github.com/tokio-rs/tokio/pull/1904
[#1898]: https://github.com/tokio-rs/tokio/pull/1898
[#1892]: https://github.com/tokio-rs/tokio/pull/1892
[#1888]: https://github.com/tokio-rs/tokio/pull/1888
[#1882]: https://github.com/tokio-rs/tokio/pull/1882
[#1881]: https://github.com/tokio-rs/tokio/pull/1881
[#1875]: https://github.com/tokio-rs/tokio/pull/1875
[#1874]: https://github.com/tokio-rs/tokio/pull/1874
[#1870]: https://github.com/tokio-rs/tokio/pull/1870
[#1864]: https://github.com/tokio-rs/tokio/pull/1864
[#1863]: https://github.com/tokio-rs/tokio/pull/1863
[#1861]: https://github.com/tokio-rs/tokio/pull/1861
[#1858]: https://github.com/tokio-rs/tokio/pull/1858
[#1856]: https://github.com/tokio-rs/tokio/pull/1856
[#1854]: https://github.com/tokio-rs/tokio/pull/1854
[#1849]: https://github.com/tokio-rs/tokio/pull/1849
[#1843]: https://github.com/tokio-rs/tokio/pull/1843
[#1841]: https://github.com/tokio-rs/tokio/pull/1841
[#1839]: https://github.com/tokio-rs/tokio/pull/1839
[#1834]: https://github.com/tokio-rs/tokio/pull/1834
[#1831]: https://github.com/tokio-rs/tokio/pull/1831
[#1827]: https://github.com/tokio-rs/tokio/pull/1827
[#1826]: https://github.com/tokio-rs/tokio/pull/1826
[#1772]: https://github.com/tokio-rs/tokio/pull/1772
[#1755]: https://github.com/tokio-rs/tokio/pull/1755
[#1733]: https://github.com/tokio-rs/tokio/pull/1733
[#1699]: https://github.com/tokio-rs/tokio/pull/1699
[#1111]: https://github.com/tokio-rs/tokio/pull/1111
[#1055]: https://github.com/tokio-rs/tokio/pull/1055
[#993]:  https://github.com/tokio-rs/tokio/pull/993
[#966]:  https://github.com/tokio-rs/tokio/pull/966
[#964]:  https://github.com/tokio-rs/tokio/pull/964
[#940]:  https://github.com/tokio-rs/tokio/pull/940
[#922]:  https://github.com/tokio-rs/tokio/pull/922
[#896]:  https://github.com/tokio-rs/tokio/pull/896
[#839]:  https://github.com/tokio-rs/tokio/pull/839
[#832]:  https://github.com/tokio-rs/tokio/pull/832
[#808]:  https://github.com/tokio-rs/tokio/pull/808
[#772]:  https://github.com/tokio-rs/tokio/pull/772
[#756]:  https://github.com/tokio-rs/tokio/pull/756
[#749]:  https://github.com/tokio-rs/tokio/pull/749
[#736]:  https://github.com/tokio-rs/tokio/pull/736
[#721]:  https://github.com/tokio-rs/tokio/pull/721
[#708]:  https://github.com/tokio-rs/tokio/pull/708
[#689]:  https://github.com/tokio-rs/tokio/pull/689
[#682]:  https://github.com/tokio-rs/tokio/pull/682
[#676]:  https://github.com/tokio-rs/tokio/pull/676
[#675]:  https://github.com/tokio-rs/tokio/pull/675
[#661]:  https://github.com/tokio-rs/tokio/pull/661
[#660]:  https://github.com/tokio-rs/tokio/pull/660
[#652]:  https://github.com/tokio-rs/tokio/pull/652
[#646]:  https://github.com/tokio-rs/tokio/pull/646
[#645]:  https://github.com/tokio-rs/tokio/pull/645
[#579]:  https://github.com/tokio-rs/tokio/pull/579
[#575]:  https://github.com/tokio-rs/tokio/pull/575
[#573]:  https://github.com/tokio-rs/tokio/pull/573
[#548]:  https://github.com/tokio-rs/tokio/pull/548
[#501]:  https://github.com/tokio-rs/tokio/pull/501
[#477]:  https://github.com/tokio-rs/tokio/pull/477
[#450]:  https://github.com/tokio-rs/tokio/pull/450
[#398]:  https://github.com/tokio-rs/tokio/pull/398
[#391]:  https://github.com/tokio-rs/tokio/pull/391
[#381]:  https://github.com/tokio-rs/tokio/pull/381
[#370]:  https://github.com/tokio-rs/tokio/pull/370
[#340]:  https://github.com/tokio-rs/tokio/pull/340
[#323]:  https://github.com/tokio-rs/tokio/pull/323
[#313]:  https://github.com/tokio-rs/tokio/pull/313
[#308]:  https://github.com/tokio-rs/tokio/pull/308
[#266]:  https://github.com/tokio-rs/tokio/pull/266
[#234]:  https://github.com/tokio-rs/tokio/pull/234
[#232]:  https://github.com/tokio-rs/tokio/pull/232
[#224]:  https://github.com/tokio-rs/tokio/pull/224
[#218]:  https://github.com/tokio-rs/tokio/pull/218
[#214]:  https://github.com/tokio-rs/tokio/pull/214
[#212]:  https://github.com/tokio-rs/tokio/pull/212
[#175]:  https://github.com/tokio-rs/tokio/pull/175
[#169]:  https://github.com/tokio-rs/tokio/pull/169
[#166]:  https://github.com/tokio-rs/tokio/pull/166
[#160]:  https://github.com/tokio-rs/tokio/pull/160
[#142]:  https://github.com/tokio-rs/tokio/pull/142
[#141]:  https://github.com/tokio-rs/tokio/pull/141
