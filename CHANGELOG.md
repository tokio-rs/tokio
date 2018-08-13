# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.2] -
### Fixes
* Fix starvation of `Signal`s whenever a `Signal` instance is dropped
* Fix starvation of individual `Signal`s based on their creation order

## [0.2.1] - 2018-05-27
### Fixes
* Bump minimum supported version of `mio` to 0.6.14

## 0.2.0 - 2018-05-07
#### Features
 * Uses `tokio` instead of `tokio_core` (#24)
 * Supports all 33 signals on FreeBSD (#27)

[Unreleased]: https://github.com/alexcrichton/tokio-process/compare/0.2.2...HEAD
[0.2.2]: https://github.com/alexcrichton/tokio-signal/compare/0.2.1...0.2.2
[0.2.1]: https://github.com/alexcrichton/tokio-signal/compare/0.2.0...0.2.1
