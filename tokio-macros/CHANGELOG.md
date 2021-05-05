# 1.1.0 (February 5, 2021)

- add `start_paused` option to macros ([#3492])

# 1.0.0 (December 23, 2020)

- track `tokio` 1.0 release.

# 0.3.1 (October 25, 2020)

### Fixed

- fix incorrect docs regarding `max_threads` option ([#3038])

# 0.3.0 (October 15, 2020)

- Track `tokio` 0.3 release.

### Changed
- options are renamed to track `tokio` runtime builder fn names.
- `#[tokio::main]` macro requires `rt-multi-thread` when no `flavor` is specified.

# 0.2.5 (February 27, 2019)

### Fixed
- doc improvements ([#2225]).

# 0.2.4 (January 27, 2019)

### Fixed
- generics on `#[tokio::main]` function ([#2177]).

### Added
- support for `tokio::select!` ([#2152]).

# 0.2.3 (January 7, 2019)

### Fixed
- Revert breaking change.

# 0.2.2 (January 7, 2019)

### Added
- General refactoring and inclusion of additional runtime options ([#2022] and [#2038])

# 0.2.1 (December 18, 2019)

### Fixes
- inherit visibility when wrapping async fn ([#1954]).

# 0.2.0 (November 26, 2019)

- Initial release

[#1954]: https://github.com/tokio-rs/tokio/pull/1954
[#2022]: https://github.com/tokio-rs/tokio/pull/2022
[#2038]: https://github.com/tokio-rs/tokio/pull/2038
[#2152]: https://github.com/tokio-rs/tokio/pull/2152
[#2177]: https://github.com/tokio-rs/tokio/pull/2177
[#2225]: https://github.com/tokio-rs/tokio/pull/2225
[#3038]: https://github.com/tokio-rs/tokio/pull/3038
[#3492]: https://github.com/tokio-rs/tokio/pull/3492
