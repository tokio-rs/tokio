# 0.2.6 (November 11, 2020)

### Changes

 - disambiguate the built-in `#[test]` attribute in macro expansion ([#2503])
 - warn about renaming the Tokio dependency ([#2521])
 - various documentation changes ([#2683], [#2697])

# 0.2.5 (February 27, 2020)

### Fixed
- doc improvements ([#2225]).

# 0.2.4 (January 27, 2020)

### Fixed
- generics on `#[tokio::main]` function ([#2177]).

### Added
- support for `tokio::select!` ([#2152]).

# 0.2.3 (January 7, 2020)

### Fixed
- Revert breaking change.

# 0.2.2 (January 7, 2020)

### Added
- General refactoring and inclusion of additional runtime options ([#2022] and [#2038])

# 0.2.1 (December 18, 2019)

### Fixes
- inherit visibility when wrapping async fn ([#1954]).

# 0.2.0 (November 26, 2019)

- Initial release

[#2697]: https://github.com/tokio-rs/tokio/pull/2697
[#2683]: https://github.com/tokio-rs/tokio/pull/2683
[#2521]: https://github.com/tokio-rs/tokio/pull/2521
[#2503]: https://github.com/tokio-rs/tokio/pull/2503
[#2225]: https://github.com/tokio-rs/tokio/pull/2225
[#2177]: https://github.com/tokio-rs/tokio/pull/2177
[#2152]: https://github.com/tokio-rs/tokio/pull/2152
[#2038]: https://github.com/tokio-rs/tokio/pull/2038
[#2022]: https://github.com/tokio-rs/tokio/pull/2022
[#1954]: https://github.com/tokio-rs/tokio/pull/1954
