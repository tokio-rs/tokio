# 1.0.2 (January 14, 2020)

### Fixed
- io: soundness in `read_to_end` (#3428).

# 1.0.1 (December 25, 2020)

This release fixes a soundness hole caused by the combination of `RwLockWriteGuard::map`
and `RwLockWriteGuard::downgrade` by removing the `map` function. This is a breaking
change, but breaking changes are allowed under our semver policy when they are required
to fix a soundness hole. (See [this RFC][semver] for more.)

Note that we have chosen not to do a deprecation cycle or similar because Tokio 1.0.0 was
released two days ago, and therefore the impact should be minimal.

Due to the soundness hole, we have also yanked Tokio version 1.0.0.

### Removed

- sync: remove `RwLockWriteGuard::map` and `RwLockWriteGuard::try_map` ([#3345])

### Fixed

- docs: remove stream feature from docs ([#3335])

[semver]: https://github.com/rust-lang/rfcs/blob/master/text/1122-language-semver.md#soundness-changes
[#3335]: https://github.com/tokio-rs/tokio/pull/3335
[#3345]: https://github.com/tokio-rs/tokio/pull/3345

# 1.0.0 (December 23, 2020)

Commit to the API and long-term support.

### Fixed

- sync: spurious wakeup in `watch` ([#3234]).

### Changed

- io: rename `AsyncFd::with_io()` to `try_io()` ([#3306])
- fs: avoid OS specific `*Ext` traits in favor of conditionally defining the fn ([#3264]).
- fs: `Sleep` is `!Unpin` ([#3278]).
- net: pass `SocketAddr` by value ([#3125]).
- net: `TcpStream::poll_peek` takes `ReadBuf` ([#3259]).
- rt: rename `runtime::Builder::max_threads()` to `max_blocking_threads()` ([#3287]).
- time: require `current_thread` runtime when calling `time::pause()` ([#3289]).

### Removed

- remove `tokio::prelude` ([#3299]).
- io: remove `AsyncFd::with_poll()` ([#3306]).
- net: remove `{Tcp,Unix}Stream::shutdown()` in favor of `AsyncWrite::shutdown()` ([#3298]).
- stream: move all stream utilities to `tokio-stream` until `Stream` is added to
  `std` ([#3277]).
- sync: mpsc `try_recv()` due to unexpected behavior ([#3263]).
- tracing: make unstable as `tracing-core` is not 1.0 yet ([#3266]).

### Added

- fs: `poll_*` fns to `DirEntry` ([#3308]).
- io: `poll_*` fns to `io::Lines`, `io::Split` ([#3308]).
- io: `_mut` method variants to `AsyncFd` ([#3304]).
- net: `poll_*` fns to `UnixDatagram` ([#3223]).
- net: `UnixStream` readiness and non-blocking ops ([#3246]).
- sync: `UnboundedReceiver::blocking_recv()` ([#3262]).
- sync: `watch::Sender::borrow()` ([#3269]).
- sync: `Semaphore::close()` ([#3065]).
- sync: `poll_recv` fns to `mpsc::Receiver`, `mpsc::UnboundedReceiver` ([#3308]).
- time: `poll_tick` fn to `time::Interval` ([#3316]).

[#3065]: https://github.com/tokio-rs/tokio/pull/3065
[#3125]: https://github.com/tokio-rs/tokio/pull/3125
[#3223]: https://github.com/tokio-rs/tokio/pull/3223
[#3234]: https://github.com/tokio-rs/tokio/pull/3234
[#3246]: https://github.com/tokio-rs/tokio/pull/3246
[#3259]: https://github.com/tokio-rs/tokio/pull/3259
[#3262]: https://github.com/tokio-rs/tokio/pull/3262
[#3263]: https://github.com/tokio-rs/tokio/pull/3263
[#3264]: https://github.com/tokio-rs/tokio/pull/3264
[#3266]: https://github.com/tokio-rs/tokio/pull/3266
[#3269]: https://github.com/tokio-rs/tokio/pull/3269
[#3277]: https://github.com/tokio-rs/tokio/pull/3277
[#3278]: https://github.com/tokio-rs/tokio/pull/3278
[#3287]: https://github.com/tokio-rs/tokio/pull/3287
[#3289]: https://github.com/tokio-rs/tokio/pull/3289
[#3298]: https://github.com/tokio-rs/tokio/pull/3298
[#3299]: https://github.com/tokio-rs/tokio/pull/3299
[#3304]: https://github.com/tokio-rs/tokio/pull/3304
[#3306]: https://github.com/tokio-rs/tokio/pull/3306
[#3308]: https://github.com/tokio-rs/tokio/pull/3308
[#3316]: https://github.com/tokio-rs/tokio/pull/3316

# 0.3.6 (December 14, 2020)

### Fixed

- rt: fix deadlock in shutdown ([#3228])
- rt: fix panic in task abort when off rt ([#3159])
- sync: make `add_permits` panic with usize::MAX >> 3 permits ([#3188])
- time: Fix race condition in timer drop ([#3229])
- watch: fix spurious wakeup ([#3244])

### Added

- example: add back udp-codec example ([#3205])
- net: add `TcpStream::into_std` ([#3189])

[#3159]: https://github.com/tokio-rs/tokio/pull/3159
[#3188]: https://github.com/tokio-rs/tokio/pull/3188
[#3189]: https://github.com/tokio-rs/tokio/pull/3189
[#3205]: https://github.com/tokio-rs/tokio/pull/3205
[#3228]: https://github.com/tokio-rs/tokio/pull/3228
[#3229]: https://github.com/tokio-rs/tokio/pull/3229
[#3244]: https://github.com/tokio-rs/tokio/pull/3244

# 0.3.5 (November 30, 2020)

### Fixed

- rt: fix `shutdown_timeout(0)` ([#3196]).
- time: fixed race condition with small sleeps ([#3069]).

### Added

- io: `AsyncFd::with_interest()` ([#3167]).
- signal: `CtrlC` stream on windows ([#3186]).

[#3069]: https://github.com/tokio-rs/tokio/pull/3069
[#3167]: https://github.com/tokio-rs/tokio/pull/3167
[#3186]: https://github.com/tokio-rs/tokio/pull/3186
[#3196]: https://github.com/tokio-rs/tokio/pull/3196

# 0.3.4 (November 18, 2020)

### Fixed

- stream: `StreamMap` `Default` impl bound ([#3093]).
- io: `AsyncFd::into_inner()` should deregister the FD ([#3104]).

### Changed

- meta: `parking_lot` feature enabled with `full` ([#3119]).

### Added

- io: `AsyncWrite` vectored writes ([#3149]).
- net: TCP/UDP readiness and non-blocking ops ([#3130], [#2743], [#3138]).
- net: TCP socket option (linger, send/recv buf size) ([#3145], [#3143]).
- net: PID field in `UCred` with solaris/illumos ([#3085]).
- rt: `runtime::Handle` allows spawning onto a runtime ([#3079]).
- sync: `Notify::notify_waiters()` ([#3098]).
- sync: `acquire_many()`, `try_acquire_many()` to `Semaphore` ([#3067]).

[#2743]: https://github.com/tokio-rs/tokio/pull/2743
[#3067]: https://github.com/tokio-rs/tokio/pull/3067
[#3079]: https://github.com/tokio-rs/tokio/pull/3079
[#3085]: https://github.com/tokio-rs/tokio/pull/3085
[#3093]: https://github.com/tokio-rs/tokio/pull/3093
[#3098]: https://github.com/tokio-rs/tokio/pull/3098
[#3104]: https://github.com/tokio-rs/tokio/pull/3104
[#3119]: https://github.com/tokio-rs/tokio/pull/3119
[#3130]: https://github.com/tokio-rs/tokio/pull/3130
[#3138]: https://github.com/tokio-rs/tokio/pull/3138
[#3143]: https://github.com/tokio-rs/tokio/pull/3143
[#3145]: https://github.com/tokio-rs/tokio/pull/3145
[#3149]: https://github.com/tokio-rs/tokio/pull/3149

# 0.3.3 (November 2, 2020)

Fixes a soundness hole by adding a missing `Send` bound to
`Runtime::spawn_blocking()`.

### Fixed

- rt: include missing `Send`, fixing soundness hole ([#3089]).
- tracing: avoid huge trace span names ([#3074]).

### Added

- net: `TcpSocket::reuseport()`, `TcpSocket::set_reuseport()` ([#3083]).
- net: `TcpSocket::reuseaddr()` ([#3093]).
- net: `TcpSocket::local_addr()` ([#3093]).
- net: add pid to `UCred` ([#2633]).

[#2633]: https://github.com/tokio-rs/tokio/pull/2633
[#3074]: https://github.com/tokio-rs/tokio/pull/3074
[#3083]: https://github.com/tokio-rs/tokio/pull/3083
[#3089]: https://github.com/tokio-rs/tokio/pull/3089
[#3093]: https://github.com/tokio-rs/tokio/pull/3093

# 0.3.2 (October 27, 2020)

Adds `AsyncFd` as a replacement for v0.2's `PollEvented`.

### Fixed

- io: fix a potential deadlock when shutting down the I/O driver ([#2903]).
- sync: `RwLockWriteGuard::downgrade()` bug ([#2957]).

### Added

- io: `AsyncFd` for receiving readiness events on raw FDs ([#2903]).
- net: `poll_*` function on `UdpSocket` ([#2981]).
- net: `UdpSocket::take_error()` ([#3051]).
- sync: `oneshot::Sender::poll_closed()` ([#3032]).

[#2903]: https://github.com/tokio-rs/tokio/pull/2903
[#2957]: https://github.com/tokio-rs/tokio/pull/2957
[#2981]: https://github.com/tokio-rs/tokio/pull/2981
[#3032]: https://github.com/tokio-rs/tokio/pull/3032
[#3051]: https://github.com/tokio-rs/tokio/pull/3051

# 0.3.1 (October 21, 2020)

This release fixes an use-after-free in the IO driver. Additionally, the `read_buf`
and `write_buf` methods have been added back to the IO traits, as the bytes crate
is now on track to reach version 1.0 together with Tokio.

### Fixed

- net: fix use-after-free ([#3019]).
- fs: ensure buffered data is written on shutdown ([#3009]).

### Added

- io: `copy_buf()` ([#2884]).
- io: `AsyncReadExt::read_buf()`, `AsyncReadExt::write_buf()` for working with
  `Buf`/`BufMut` ([#3003]).
- rt: `Runtime::spawn_blocking()` ([#2980]).
- sync: `watch::Sender::is_closed()` ([#2991]).

[#2884]: https://github.com/tokio-rs/tokio/pull/2884
[#2980]: https://github.com/tokio-rs/tokio/pull/2980
[#2991]: https://github.com/tokio-rs/tokio/pull/2991
[#3003]: https://github.com/tokio-rs/tokio/pull/3003
[#3009]: https://github.com/tokio-rs/tokio/pull/3009
[#3019]: https://github.com/tokio-rs/tokio/pull/3019

# 0.3.0 (October 15, 2020)

This represents a 1.0 beta release. APIs are polished and future-proofed. APIs
not included for 1.0 stabilization have been removed.

Biggest changes are:

- I/O driver internal rewrite. The windows implementation includes significant
  changes.
- Runtime API is polished, especially with how it interacts with feature flag
  combinations.
- Feature flags are simplified
  - `rt-core` and `rt-util` are combined to `rt`
  - `rt-threaded` is renamed to `rt-multi-thread` to match builder API
  - `tcp`, `udp`, `uds`, `dns` are combied to `net`.
  - `parking_lot` is included with `full`

### Changes

- meta: Minimum supported Rust version is now 1.45.
- io: `AsyncRead` trait now takes `ReadBuf` in order to safely handle reading
  into uninitialized memory ([#2758]).
- io: Internal I/O driver storage is now able to compact ([#2757]).
- rt: `Runtime::block_on` now takes `&self` ([#2782]).
- sync: `watch` reworked to decouple receiving a change notification from
  receiving the value ([#2814], [#2806]).
- sync: `Notify::notify` is renamed to `notify_one` ([#2822]).
- process: `Child::kill` is now an `async fn` that cleans zombies ([#2823]).
- sync: use `const fn` constructors as possible ([#2833], [#2790])
- signal: reduce cross-thread notification ([#2835]).
- net: tcp,udp,uds types support operations with `&self` ([#2828], [#2919], [#2934]).
- sync: blocking `mpsc` channel supports `send` with `&self` ([#2861]).
- time: rename `delay_for` and `delay_until` to `sleep` and `sleep_until` ([#2826]).
- io: upgrade to `mio` 0.7 ([#2893]).
- io: `AsyncSeek` trait is tweaked ([#2885]).
- fs: `File` operations take `&self` ([#2930]).
- rt: runtime API, and `#[tokio::main]` macro polish ([#2876])
- rt: `Runtime::enter` uses an RAII guard instead of a closure ([#2954]).
- net: the `from_std` function on all sockets no longer sets socket into non-blocking mode ([#2893])

### Added

- sync: `map` function to lock guards ([#2445]).
- sync: `blocking_recv` and `blocking_send` fns to `mpsc` for use outside of Tokio ([#2685]).
- rt: `Builder::thread_name_fn` for configuring thread names ([#1921]).
- fs: impl `FromRawFd` and `FromRawHandle` for `File` ([#2792]).
- process: `Child::wait` and `Child::try_wait` ([#2796]).
- rt: support configuring thread keep-alive duration ([#2809]).
- rt: `task::JoinHandle::abort` forcibly cancels a spawned task ([#2474]).
- sync: `RwLock` write guard to read guard downgrading ([#2733]).
- net: add `poll_*` functions that take `&self` to all net types ([#2845])
- sync: `get_mut()` for `Mutex`, `RwLock` ([#2856]).
- sync: `mpsc::Sender::closed()` waits for `Receiver` half to close ([#2840]).
- sync: `mpsc::Sender::is_closed()` returns true if `Receiver` half is closed ([#2726]).
- stream: `iter` and `iter_mut` to `StreamMap` ([#2890]).
- net: implement `AsRawSocket` on windows ([#2911]).
- net: `TcpSocket` creates a socket without binding or listening ([#2920]).

### Removed

- io: vectored ops are removed from `AsyncRead`, `AsyncWrite` traits ([#2882]).
- io: `mio` is removed from the public API. `PollEvented` and` Registration` are
  removed ([#2893]).
- io: remove `bytes` from public API. `Buf` and `BufMut` implementation are
  removed ([#2908]).
- time: `DelayQueue` is moved to `tokio-util` ([#2897]).

### Fixed

- io: `stdout` and `stderr` buffering on windows ([#2734]).

[#1921]: https://github.com/tokio-rs/tokio/pull/1921
[#2445]: https://github.com/tokio-rs/tokio/pull/2445
[#2474]: https://github.com/tokio-rs/tokio/pull/2474
[#2685]: https://github.com/tokio-rs/tokio/pull/2685
[#2726]: https://github.com/tokio-rs/tokio/pull/2726
[#2733]: https://github.com/tokio-rs/tokio/pull/2733
[#2734]: https://github.com/tokio-rs/tokio/pull/2734
[#2757]: https://github.com/tokio-rs/tokio/pull/2757
[#2758]: https://github.com/tokio-rs/tokio/pull/2758
[#2782]: https://github.com/tokio-rs/tokio/pull/2782
[#2790]: https://github.com/tokio-rs/tokio/pull/2790
[#2792]: https://github.com/tokio-rs/tokio/pull/2792
[#2796]: https://github.com/tokio-rs/tokio/pull/2796
[#2806]: https://github.com/tokio-rs/tokio/pull/2806
[#2809]: https://github.com/tokio-rs/tokio/pull/2809
[#2814]: https://github.com/tokio-rs/tokio/pull/2814
[#2822]: https://github.com/tokio-rs/tokio/pull/2822
[#2823]: https://github.com/tokio-rs/tokio/pull/2823
[#2826]: https://github.com/tokio-rs/tokio/pull/2826
[#2828]: https://github.com/tokio-rs/tokio/pull/2828
[#2833]: https://github.com/tokio-rs/tokio/pull/2833
[#2835]: https://github.com/tokio-rs/tokio/pull/2835
[#2840]: https://github.com/tokio-rs/tokio/pull/2840
[#2845]: https://github.com/tokio-rs/tokio/pull/2845
[#2856]: https://github.com/tokio-rs/tokio/pull/2856
[#2861]: https://github.com/tokio-rs/tokio/pull/2861
[#2876]: https://github.com/tokio-rs/tokio/pull/2876
[#2882]: https://github.com/tokio-rs/tokio/pull/2882
[#2885]: https://github.com/tokio-rs/tokio/pull/2885
[#2890]: https://github.com/tokio-rs/tokio/pull/2890
[#2893]: https://github.com/tokio-rs/tokio/pull/2893
[#2897]: https://github.com/tokio-rs/tokio/pull/2897
[#2908]: https://github.com/tokio-rs/tokio/pull/2908
[#2911]: https://github.com/tokio-rs/tokio/pull/2911
[#2919]: https://github.com/tokio-rs/tokio/pull/2919
[#2920]: https://github.com/tokio-rs/tokio/pull/2920
[#2930]: https://github.com/tokio-rs/tokio/pull/2930
[#2934]: https://github.com/tokio-rs/tokio/pull/2934
[#2954]: https://github.com/tokio-rs/tokio/pull/2954

# 0.2.22 (July 21, 2020)

### Fixes

- docs: misc improvements ([#2572], [#2658], [#2663], [#2656], [#2647], [#2630], [#2487], [#2621],
  [#2624], [#2600], [#2623], [#2622], [#2577], [#2569], [#2589], [#2575], [#2540], [#2564], [#2567],
  [#2520], [#2521], [#2493])
- rt: allow calls to `block_on` inside calls to `block_in_place` that are
  themselves inside `block_on` ([#2645])
- net: fix non-portable behavior when dropping `TcpStream` `OwnedWriteHalf` ([#2597])
- io: improve stack usage by allocating large buffers on directly on the heap
  ([#2634])
- io: fix unsound pin projection in `AsyncReadExt::read_buf` and
  `AsyncWriteExt::write_buf` ([#2612])
- io: fix unnecessary zeroing for `AsyncRead` implementors ([#2525])
- io: Fix `BufReader` not correctly forwarding `poll_write_buf` ([#2654])
- io: fix panic in `AsyncReadExt::read_line` ([#2541])

### Changes

- coop: returning `Poll::Pending` no longer decrements the task budget ([#2549])

### Added

- io: little-endian variants of `AsyncReadExt` and `AsyncWriteExt` methods
  ([#1915])
- task: add [`tracing`] instrumentation to spawned tasks ([#2655])
- sync: allow unsized types in `Mutex` and `RwLock` (via `default` constructors)
  ([#2615])
- net: add `ToSocketAddrs` implementation for `&[SocketAddr]` ([#2604])
- fs: add `OpenOptionsExt` for `OpenOptions` ([#2515])
- fs: add `DirBuilder` ([#2524])

[`tracing`]: https://crates.io/crates/tracing
[#1915]: https://github.com/tokio-rs/tokio/pull/1915
[#2487]: https://github.com/tokio-rs/tokio/pull/2487
[#2493]: https://github.com/tokio-rs/tokio/pull/2493
[#2515]: https://github.com/tokio-rs/tokio/pull/2515
[#2520]: https://github.com/tokio-rs/tokio/pull/2520
[#2521]: https://github.com/tokio-rs/tokio/pull/2521
[#2524]: https://github.com/tokio-rs/tokio/pull/2524
[#2525]: https://github.com/tokio-rs/tokio/pull/2525
[#2540]: https://github.com/tokio-rs/tokio/pull/2540
[#2541]: https://github.com/tokio-rs/tokio/pull/2541
[#2549]: https://github.com/tokio-rs/tokio/pull/2549
[#2564]: https://github.com/tokio-rs/tokio/pull/2564
[#2567]: https://github.com/tokio-rs/tokio/pull/2567
[#2569]: https://github.com/tokio-rs/tokio/pull/2569
[#2572]: https://github.com/tokio-rs/tokio/pull/2572
[#2575]: https://github.com/tokio-rs/tokio/pull/2575
[#2577]: https://github.com/tokio-rs/tokio/pull/2577
[#2589]: https://github.com/tokio-rs/tokio/pull/2589
[#2597]: https://github.com/tokio-rs/tokio/pull/2597
[#2600]: https://github.com/tokio-rs/tokio/pull/2600
[#2604]: https://github.com/tokio-rs/tokio/pull/2604
[#2612]: https://github.com/tokio-rs/tokio/pull/2612
[#2615]: https://github.com/tokio-rs/tokio/pull/2615
[#2621]: https://github.com/tokio-rs/tokio/pull/2621
[#2622]: https://github.com/tokio-rs/tokio/pull/2622
[#2623]: https://github.com/tokio-rs/tokio/pull/2623
[#2624]: https://github.com/tokio-rs/tokio/pull/2624
[#2630]: https://github.com/tokio-rs/tokio/pull/2630
[#2634]: https://github.com/tokio-rs/tokio/pull/2634
[#2645]: https://github.com/tokio-rs/tokio/pull/2645
[#2647]: https://github.com/tokio-rs/tokio/pull/2647
[#2654]: https://github.com/tokio-rs/tokio/pull/2654
[#2655]: https://github.com/tokio-rs/tokio/pull/2655
[#2656]: https://github.com/tokio-rs/tokio/pull/2656
[#2658]: https://github.com/tokio-rs/tokio/pull/2658
[#2663]: https://github.com/tokio-rs/tokio/pull/2663

# 0.2.21 (May 13, 2020)

### Fixes

- macros: disambiguate built-in `#[test]` attribute in macro expansion ([#2503])
- rt: `LocalSet` and task budgeting ([#2462]).
- rt: task budgeting with `block_in_place` ([#2502]).
- sync: release `broadcast` channel memory without sending a value ([#2509]).
- time: notify when resetting a `Delay` to a time in the past ([#2290])

### Added

- io: `get_mut`, `get_ref`, and `into_inner` to `Lines` ([#2450]).
- io: `mio::Ready` argument to `PollEvented` ([#2419]).
- os: illumos support ([#2486]).
- rt: `Handle::spawn_blocking` ([#2501]).
- sync: `OwnedMutexGuard` for `Arc<Mutex<T>>` ([#2455]).

[#2290]: https://github.com/tokio-rs/tokio/pull/2290
[#2419]: https://github.com/tokio-rs/tokio/pull/2419
[#2450]: https://github.com/tokio-rs/tokio/pull/2450
[#2455]: https://github.com/tokio-rs/tokio/pull/2455
[#2462]: https://github.com/tokio-rs/tokio/pull/2462
[#2486]: https://github.com/tokio-rs/tokio/pull/2486
[#2501]: https://github.com/tokio-rs/tokio/pull/2501
[#2502]: https://github.com/tokio-rs/tokio/pull/2502
[#2503]: https://github.com/tokio-rs/tokio/pull/2503
[#2509]: https://github.com/tokio-rs/tokio/pull/2509

# 0.2.20 (April 28, 2020)

### Fixes

- sync: `broadcast` closing the channel no longer requires capacity ([#2448]).
- rt: regression when configuring runtime with `max_threads` less than number of CPUs ([#2457]).

[#2448]: https://github.com/tokio-rs/tokio/pull/2448
[#2457]: https://github.com/tokio-rs/tokio/pull/2457

# 0.2.19 (April 24, 2020)

### Fixes

- docs: misc improvements ([#2400], [#2405], [#2414], [#2420], [#2423], [#2426], [#2427], [#2434], [#2436], [#2440]).
- rt: support `block_in_place` in more contexts ([#2409], [#2410]).
- stream: no panic in `merge()` and `chain()` when using `size_hint()` ([#2430]).
- task: include visibility modifier when defining a task-local ([#2416]).

### Added

- rt: `runtime::Handle::block_on` ([#2437]).
- sync: owned `Semaphore` permit ([#2421]).
- tcp: owned split ([#2270]).

[#2270]: https://github.com/tokio-rs/tokio/pull/2270
[#2400]: https://github.com/tokio-rs/tokio/pull/2400
[#2405]: https://github.com/tokio-rs/tokio/pull/2405
[#2409]: https://github.com/tokio-rs/tokio/pull/2409
[#2410]: https://github.com/tokio-rs/tokio/pull/2410
[#2414]: https://github.com/tokio-rs/tokio/pull/2414
[#2416]: https://github.com/tokio-rs/tokio/pull/2416
[#2420]: https://github.com/tokio-rs/tokio/pull/2420
[#2421]: https://github.com/tokio-rs/tokio/pull/2421
[#2423]: https://github.com/tokio-rs/tokio/pull/2423
[#2426]: https://github.com/tokio-rs/tokio/pull/2426
[#2427]: https://github.com/tokio-rs/tokio/pull/2427
[#2430]: https://github.com/tokio-rs/tokio/pull/2430
[#2434]: https://github.com/tokio-rs/tokio/pull/2434
[#2436]: https://github.com/tokio-rs/tokio/pull/2436
[#2437]: https://github.com/tokio-rs/tokio/pull/2437
[#2440]: https://github.com/tokio-rs/tokio/pull/2440

# 0.2.18 (April 12, 2020)

### Fixes

- task: `LocalSet` was incorrectly marked as `Send` ([#2398])
- io: correctly report `WriteZero` failure in `write_int` ([#2334])

[#2334]: https://github.com/tokio-rs/tokio/pull/2334
[#2398]: https://github.com/tokio-rs/tokio/pull/2398

# 0.2.17 (April 9, 2020)

### Fixes

- rt: bug in work-stealing queue ([#2387])

### Changes

- rt: threadpool uses logical CPU count instead of physical by default ([#2391])

[#2387]: https://github.com/tokio-rs/tokio/pull/2387
[#2391]: https://github.com/tokio-rs/tokio/pull/2391

# 0.2.16 (April 3, 2020)

### Fixes

- sync: fix a regression where `Mutex`, `Semaphore`, and `RwLock` futures no
  longer implement `Sync` ([#2375])
- fs: fix `fs::copy` not copying file permissions ([#2354])

### Added

- time: added `deadline` method to `delay_queue::Expired` ([#2300])
- io: added `StreamReader` ([#2052])

[#2052]: https://github.com/tokio-rs/tokio/pull/2052
[#2300]: https://github.com/tokio-rs/tokio/pull/2300
[#2354]: https://github.com/tokio-rs/tokio/pull/2354
[#2375]: https://github.com/tokio-rs/tokio/pull/2375

# 0.2.15 (April 2, 2020)

### Fixes

- rt: fix queue regression ([#2362]).

### Added

- sync: Add disarm to `mpsc::Sender` ([#2358]).

[#2358]: https://github.com/tokio-rs/tokio/pull/2358
[#2362]: https://github.com/tokio-rs/tokio/pull/2362

# 0.2.14 (April 1, 2020)

### Fixes

- rt: concurrency bug in scheduler ([#2273]).
- rt: concurrency bug with shell runtime ([#2333]).
- test-util: correct pause/resume of time ([#2253]).
- time: `DelayQueue` correct wakeup after `insert` ([#2285]).

### Added

- io: impl `RawFd`, `AsRawHandle` for std io types ([#2335]).
- rt: automatic cooperative task yielding ([#2160], [#2343], [#2349]).
- sync: `RwLock::into_inner` ([#2321]).

### Changed

- sync: semaphore, mutex internals rewritten to avoid allocations ([#2325]).

[#2160]: https://github.com/tokio-rs/tokio/pull/2160
[#2253]: https://github.com/tokio-rs/tokio/pull/2253
[#2273]: https://github.com/tokio-rs/tokio/pull/2273
[#2285]: https://github.com/tokio-rs/tokio/pull/2285
[#2321]: https://github.com/tokio-rs/tokio/pull/2321
[#2325]: https://github.com/tokio-rs/tokio/pull/2325
[#2333]: https://github.com/tokio-rs/tokio/pull/2333
[#2335]: https://github.com/tokio-rs/tokio/pull/2335
[#2343]: https://github.com/tokio-rs/tokio/pull/2343
[#2349]: https://github.com/tokio-rs/tokio/pull/2349

# 0.2.13 (February 28, 2020)

### Fixes

- macros: unresolved import in `pin!` ([#2281]).

[#2281]: https://github.com/tokio-rs/tokio/pull/2281

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

[#2119]: https://github.com/tokio-rs/tokio/pull/2119
[#2184]: https://github.com/tokio-rs/tokio/pull/2184
[#2185]: https://github.com/tokio-rs/tokio/pull/2185
[#2186]: https://github.com/tokio-rs/tokio/pull/2186
[#2191]: https://github.com/tokio-rs/tokio/pull/2191
[#2204]: https://github.com/tokio-rs/tokio/pull/2204
[#2205]: https://github.com/tokio-rs/tokio/pull/2205
[#2210]: https://github.com/tokio-rs/tokio/pull/2210
[#2217]: https://github.com/tokio-rs/tokio/pull/2217
[#2218]: https://github.com/tokio-rs/tokio/pull/2218
[#2227]: https://github.com/tokio-rs/tokio/pull/2227
[#2238]: https://github.com/tokio-rs/tokio/pull/2238
[#2239]: https://github.com/tokio-rs/tokio/pull/2239
[#2245]: https://github.com/tokio-rs/tokio/pull/2245
[#2250]: https://github.com/tokio-rs/tokio/pull/2250
[#2274]: https://github.com/tokio-rs/tokio/pull/2274
[#2275]: https://github.com/tokio-rs/tokio/pull/2275

# 0.2.11 (January 27, 2020)

### Fixes

- docs: misc fixes and tweaks ([#2155], [#2103], [#2027], [#2167], [#2175]).
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

[#2027]: https://github.com/tokio-rs/tokio/pull/2027
[#2051]: https://github.com/tokio-rs/tokio/pull/2051
[#2079]: https://github.com/tokio-rs/tokio/pull/2079
[#2103]: https://github.com/tokio-rs/tokio/pull/2103
[#2122]: https://github.com/tokio-rs/tokio/pull/2122
[#2135]: https://github.com/tokio-rs/tokio/pull/2135
[#2145]: https://github.com/tokio-rs/tokio/pull/2145
[#2149]: https://github.com/tokio-rs/tokio/pull/2149
[#2151]: https://github.com/tokio-rs/tokio/pull/2151
[#2152]: https://github.com/tokio-rs/tokio/pull/2152
[#2155]: https://github.com/tokio-rs/tokio/pull/2155
[#2158]: https://github.com/tokio-rs/tokio/pull/2158
[#2163]: https://github.com/tokio-rs/tokio/pull/2163
[#2164]: https://github.com/tokio-rs/tokio/pull/2164
[#2167]: https://github.com/tokio-rs/tokio/pull/2167
[#2168]: https://github.com/tokio-rs/tokio/pull/2168
[#2169]: https://github.com/tokio-rs/tokio/pull/2169
[#2175]: https://github.com/tokio-rs/tokio/pull/2175
[#2177]: https://github.com/tokio-rs/tokio/pull/2177

# 0.2.10 (January 21, 2020)

### Fixes

- `#[tokio::main]` when `rt-core` feature flag is not enabled ([#2139]).
- remove `AsyncBufRead` from `BufStream` impl block ([#2108]).
- potential undefined behavior when implementing `AsyncRead` incorrectly ([#2030]).

### Added

- `BufStream::with_capacity` ([#2125]).
- impl `From` and `Default` for `RwLock` ([#2089]).
- `io::ReadHalf::is_pair_of` checks if provided `WriteHalf` is for the same
  underlying object ([#1762], [#2144]).
- `runtime::Handle::try_current()` returns a handle to the current runtime ([#2118]).
- `stream::empty()` returns an immediately ready empty stream ([#2092]).
- `stream::once(val)` returns a stream that yields a single value: `val` ([#2094]).
- `stream::pending()` returns a stream that never becomes ready ([#2092]).
- `StreamExt::chain()` sequences a second stream after the first completes ([#2093]).
- `StreamExt::collect()` transform a stream into a collection ([#2109]).
- `StreamExt::fuse` ends the stream after the first `None` ([#2085]).
- `StreamExt::merge` combines two streams, yielding values as they become ready ([#2091]).
- Task-local storage ([#2126]).

[#1762]: https://github.com/tokio-rs/tokio/pull/1762
[#2030]: https://github.com/tokio-rs/tokio/pull/2030
[#2085]: https://github.com/tokio-rs/tokio/pull/2085
[#2089]: https://github.com/tokio-rs/tokio/pull/2089
[#2091]: https://github.com/tokio-rs/tokio/pull/2091
[#2092]: https://github.com/tokio-rs/tokio/pull/2092
[#2093]: https://github.com/tokio-rs/tokio/pull/2093
[#2094]: https://github.com/tokio-rs/tokio/pull/2094
[#2108]: https://github.com/tokio-rs/tokio/pull/2108
[#2109]: https://github.com/tokio-rs/tokio/pull/2109
[#2118]: https://github.com/tokio-rs/tokio/pull/2118
[#2125]: https://github.com/tokio-rs/tokio/pull/2125
[#2126]: https://github.com/tokio-rs/tokio/pull/2126
[#2139]: https://github.com/tokio-rs/tokio/pull/2139
[#2144]: https://github.com/tokio-rs/tokio/pull/2144

# 0.2.9 (January 9, 2020)

### Fixes

- `AsyncSeek` impl for `File` ([#1986]).
- rt: shutdown deadlock in `threaded_scheduler` ([#2074], [#2082]).
- rt: memory ordering when dropping `JoinHandle` ([#2044]).
- docs: misc API documentation fixes and improvements.

[#1986]: https://github.com/tokio-rs/tokio/pull/1986
[#2044]: https://github.com/tokio-rs/tokio/pull/2044
[#2074]: https://github.com/tokio-rs/tokio/pull/2074
[#2082]: https://github.com/tokio-rs/tokio/pull/2082

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

[#1699]: https://github.com/tokio-rs/tokio/pull/1699
[#1755]: https://github.com/tokio-rs/tokio/pull/1755
[#1870]: https://github.com/tokio-rs/tokio/pull/1870
[#1971]: https://github.com/tokio-rs/tokio/pull/1971
[#2001]: https://github.com/tokio-rs/tokio/pull/2001
[#2005]: https://github.com/tokio-rs/tokio/pull/2005
[#2006]: https://github.com/tokio-rs/tokio/pull/2006
[#2011]: https://github.com/tokio-rs/tokio/pull/2011
[#2012]: https://github.com/tokio-rs/tokio/pull/2012
[#2014]: https://github.com/tokio-rs/tokio/pull/2014
[#2022]: https://github.com/tokio-rs/tokio/pull/2022
[#2025]: https://github.com/tokio-rs/tokio/pull/2025
[#2029]: https://github.com/tokio-rs/tokio/pull/2029
[#2034]: https://github.com/tokio-rs/tokio/pull/2034
[#2035]: https://github.com/tokio-rs/tokio/pull/2035
[#2040]: https://github.com/tokio-rs/tokio/pull/2040
[#2045]: https://github.com/tokio-rs/tokio/pull/2045
[#2059]: https://github.com/tokio-rs/tokio/pull/2059

# 0.2.6 (December 19, 2019)

### Fixes

- `fs::File::seek` API regression ([#1991]).

[#1991]: https://github.com/tokio-rs/tokio/pull/1991

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
- misc doc improvements ([#1933], [#1934], [#1940], [#1942]).

### Changes

- runtime threads are configured with `runtime::Builder::core_threads` and
  `runtime::Builder::max_threads`. `runtime::Builder::num_threads` is
  deprecated ([#1977]).

[#1924]: https://github.com/tokio-rs/tokio/pull/1924
[#1933]: https://github.com/tokio-rs/tokio/pull/1933
[#1934]: https://github.com/tokio-rs/tokio/pull/1934
[#1939]: https://github.com/tokio-rs/tokio/pull/1939
[#1940]: https://github.com/tokio-rs/tokio/pull/1940
[#1942]: https://github.com/tokio-rs/tokio/pull/1942
[#1943]: https://github.com/tokio-rs/tokio/pull/1943
[#1949]: https://github.com/tokio-rs/tokio/pull/1949
[#1956]: https://github.com/tokio-rs/tokio/pull/1956
[#1961]: https://github.com/tokio-rs/tokio/pull/1961
[#1962]: https://github.com/tokio-rs/tokio/pull/1962
[#1972]: https://github.com/tokio-rs/tokio/pull/1972
[#1973]: https://github.com/tokio-rs/tokio/pull/1973
[#1975]: https://github.com/tokio-rs/tokio/pull/1975
[#1977]: https://github.com/tokio-rs/tokio/pull/1977
[#1978]: https://github.com/tokio-rs/tokio/pull/1978

# 0.2.4 (December 6, 2019)

### Fixes

- `sync::Mutex` deadlock when `lock()` future is dropped early ([#1898]).

[#1898]: https://github.com/tokio-rs/tokio/pull/1898

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
- improved documentation ([#1865], [#1866], [#1868], [#1874], [#1876], [#1911]).

[#1863]: https://github.com/tokio-rs/tokio/pull/1863
[#1864]: https://github.com/tokio-rs/tokio/pull/1864
[#1865]: https://github.com/tokio-rs/tokio/pull/1865
[#1866]: https://github.com/tokio-rs/tokio/pull/1866
[#1868]: https://github.com/tokio-rs/tokio/pull/1868
[#1874]: https://github.com/tokio-rs/tokio/pull/1874
[#1875]: https://github.com/tokio-rs/tokio/pull/1875
[#1876]: https://github.com/tokio-rs/tokio/pull/1876
[#1881]: https://github.com/tokio-rs/tokio/pull/1881
[#1882]: https://github.com/tokio-rs/tokio/pull/1882
[#1888]: https://github.com/tokio-rs/tokio/pull/1888
[#1892]: https://github.com/tokio-rs/tokio/pull/1892
[#1904]: https://github.com/tokio-rs/tokio/pull/1904
[#1905]: https://github.com/tokio-rs/tokio/pull/1905
[#1911]: https://github.com/tokio-rs/tokio/pull/1911

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

[#1772]: https://github.com/tokio-rs/tokio/pull/1772
[#1834]: https://github.com/tokio-rs/tokio/pull/1834
[#1839]: https://github.com/tokio-rs/tokio/pull/1839
[#1841]: https://github.com/tokio-rs/tokio/pull/1841
[#1843]: https://github.com/tokio-rs/tokio/pull/1843
[#1849]: https://github.com/tokio-rs/tokio/pull/1849
[#1854]: https://github.com/tokio-rs/tokio/pull/1854
[#1856]: https://github.com/tokio-rs/tokio/pull/1856
[#1858]: https://github.com/tokio-rs/tokio/pull/1858
[#1861]: https://github.com/tokio-rs/tokio/pull/1861

# 0.2.1 (November 26, 2019)

### Fixes

- API docs for `TcpListener::incoming`, `UnixListener::incoming` ([#1831]).

### Added

- `tokio::task::LocalSet` provides a strategy for spawning `!Send` tasks ([#1733]).
- export `tokio::time::Elapsed` ([#1826]).
- impl `AsRawFd`, `AsRawHandle` for `tokio::fs::File` ([#1827]).

[#1733]: https://github.com/tokio-rs/tokio/pull/1733
[#1826]: https://github.com/tokio-rs/tokio/pull/1826
[#1827]: https://github.com/tokio-rs/tokio/pull/1827
[#1831]: https://github.com/tokio-rs/tokio/pull/1831

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

[#1111]: https://github.com/tokio-rs/tokio/pull/1111

# 0.1.20 (May 14, 2019)

### Added

- `tokio::runtime::Builder::panic_handler` allows configuring handling
  panics on the runtime ([#1055]).

[#1055]: https://github.com/tokio-rs/tokio/pull/1055

# 0.1.19 (April 22, 2019)

### Added

- Re-export `tokio::sync::Mutex` primitive ([#964]).

[#964]: https://github.com/tokio-rs/tokio/pull/964

# 0.1.18 (March 22, 2019)

### Added

- `TypedExecutor` re-export and implementations ([#993]).

[#993]: https://github.com/tokio-rs/tokio/pull/993

# 0.1.17 (March 13, 2019)

### Added

- Propagate trace subscriber in the runtime ([#966]).

[#966]: https://github.com/tokio-rs/tokio/pull/966

# 0.1.16 (March 1, 2019)

### Fixed

- async-await: track latest nightly changes ([#940]).

### Added

- `sync::Watch`, a single value broadcast channel ([#922]).
- Async equivalent of read / write file helpers being added to `std` ([#896]).

[#896]: https://github.com/tokio-rs/tokio/pull/896
[#922]: https://github.com/tokio-rs/tokio/pull/922
[#940]: https://github.com/tokio-rs/tokio/pull/940

# 0.1.15 (January 24, 2019)

### Added

- Re-export tokio-sync APIs ([#839]).
- Stream enumerate combinator ([#832]).

[#832]: https://github.com/tokio-rs/tokio/pull/832
[#839]: https://github.com/tokio-rs/tokio/pull/839

# 0.1.14 (January 6, 2019)

- Use feature flags to break up the crate, allowing users to pick & choose
  components ([#808]).
- Export `UnixDatagram` and `UnixDatagramFramed` ([#772]).

[#772]: https://github.com/tokio-rs/tokio/pull/772
[#808]: https://github.com/tokio-rs/tokio/pull/808

# 0.1.13 (November 21, 2018)

- Fix `Runtime::reactor()` when no tasks are spawned ([#721]).
- `runtime::Builder` no longer uses deprecated methods ([#749]).
- Provide `after_start` and `before_stop` configuration settings for
  `Runtime` ([#756]).
- Implement throttle stream combinator ([#736]).

[#721]: https://github.com/tokio-rs/tokio/pull/721
[#736]: https://github.com/tokio-rs/tokio/pull/736
[#749]: https://github.com/tokio-rs/tokio/pull/749
[#756]: https://github.com/tokio-rs/tokio/pull/756

# 0.1.12 (October 23, 2018)

- runtime: expose `keep_alive` on runtime builder ([#676]).
- runtime: create a reactor per worker thread ([#660]).
- codec: fix panic in `LengthDelimitedCodec` ([#682]).
- io: re-export `tokio_io::io::read` function ([#689]).
- runtime: check for executor re-entry in more places ([#708]).

[#660]: https://github.com/tokio-rs/tokio/pull/660
[#676]: https://github.com/tokio-rs/tokio/pull/676
[#682]: https://github.com/tokio-rs/tokio/pull/682
[#689]: https://github.com/tokio-rs/tokio/pull/689
[#708]: https://github.com/tokio-rs/tokio/pull/708

# 0.1.11 (September 28, 2018)

- Fix `tokio-async-await` dependency ([#675]).

[#675]: https://github.com/tokio-rs/tokio/pull/675

# 0.1.10 (September 27, 2018)

- Fix minimal versions

# 0.1.9 (September 27, 2018)

- Experimental async/await improvements ([#661]).
- Re-export `TaskExecutor` from `tokio-current-thread` ([#652]).
- Improve `Runtime` builder API ([#645]).
- `tokio::run` panics when called from the context of an executor
  ([#646]).
- Introduce `StreamExt` with a `timeout` helper ([#573]).
- Move `length_delimited` into `tokio` ([#575]).
- Re-organize `tokio::net` module ([#548]).
- Re-export `tokio-current-thread::spawn` in current_thread runtime
  ([#579]).

[#548]: https://github.com/tokio-rs/tokio/pull/548
[#573]: https://github.com/tokio-rs/tokio/pull/573
[#575]: https://github.com/tokio-rs/tokio/pull/575
[#579]: https://github.com/tokio-rs/tokio/pull/579
[#645]: https://github.com/tokio-rs/tokio/pull/645
[#646]: https://github.com/tokio-rs/tokio/pull/646
[#652]: https://github.com/tokio-rs/tokio/pull/652
[#661]: https://github.com/tokio-rs/tokio/pull/661

# 0.1.8 (August 23, 2018)

- Extract tokio::executor::current_thread to a sub crate ([#370])
- Add `Runtime::block_on` ([#398])
- Add `runtime::current_thread::block_on_all` ([#477])
- Misc documentation improvements ([#450])
- Implement `std::error::Error` for error types ([#501])

[#370]: https://github.com/tokio-rs/tokio/pull/370
[#398]: https://github.com/tokio-rs/tokio/pull/398
[#450]: https://github.com/tokio-rs/tokio/pull/450
[#477]: https://github.com/tokio-rs/tokio/pull/477
[#501]: https://github.com/tokio-rs/tokio/pull/501

# 0.1.7 (June 6, 2018)

- Add `Runtime::block_on` for concurrent runtime ([#391]).
- Provide handle to `current_thread::Runtime` that allows spawning tasks from
  other threads ([#340]).
- Provide `clock::now()`, a configurable source of time ([#381]).

[#340]: https://github.com/tokio-rs/tokio/pull/340
[#381]: https://github.com/tokio-rs/tokio/pull/381
[#391]: https://github.com/tokio-rs/tokio/pull/391

# 0.1.6 (May 2, 2018)

- Add asynchronous filesystem APIs ([#323]).
- Add "current thread" runtime variant ([#308]).
- `CurrentThread`: Expose inner `Park` instance.
- Improve fairness of `CurrentThread` executor ([#313]).

[#308]: https://github.com/tokio-rs/tokio/pull/308
[#313]: https://github.com/tokio-rs/tokio/pull/313
[#323]: https://github.com/tokio-rs/tokio/pull/323

# 0.1.5 (March 30, 2018)

- Provide timer API ([#266])

[#266]: https://github.com/tokio-rs/tokio/pull/266

# 0.1.4 (March 22, 2018)

- Fix build on FreeBSD ([#218])
- Shutdown the Runtime when the handle is dropped ([#214])
- Set Runtime thread name prefix for worker threads ([#232])
- Add builder for Runtime ([#234])
- Extract TCP and UDP types into separate crates ([#224])
- Optionally support futures 0.2.

[#214]: https://github.com/tokio-rs/tokio/pull/214
[#218]: https://github.com/tokio-rs/tokio/pull/218
[#224]: https://github.com/tokio-rs/tokio/pull/224
[#232]: https://github.com/tokio-rs/tokio/pull/232
[#234]: https://github.com/tokio-rs/tokio/pull/234

# 0.1.3 (March 09, 2018)

- Fix `CurrentThread::turn` to block on idle ([#212]).

[#212]: https://github.com/tokio-rs/tokio/pull/212

# 0.1.2 (March 09, 2018)

- Introduce Tokio Runtime ([#141])
- Provide `CurrentThread` for more flexible usage of current thread executor ([#141]).
- Add Lio for platforms that support it ([#142]).
- I/O resources now lazily bind to the reactor ([#160]).
- Extract Reactor to dedicated crate ([#169])
- Add facade to sub crates and add prelude ([#166]).
- Switch TCP/UDP fns to poll\_ -> Poll<...> style ([#175])

[#141]: https://github.com/tokio-rs/tokio/pull/141
[#142]: https://github.com/tokio-rs/tokio/pull/142
[#160]: https://github.com/tokio-rs/tokio/pull/160
[#166]: https://github.com/tokio-rs/tokio/pull/166
[#169]: https://github.com/tokio-rs/tokio/pull/169
[#175]: https://github.com/tokio-rs/tokio/pull/175

# 0.1.1 (February 09, 2018)

- Doc fixes

# 0.1.0 (February 07, 2018)

- Initial crate released based on [RFC](https://github.com/tokio-rs/tokio-rfcs/pull/3).
