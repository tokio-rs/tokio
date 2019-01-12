# 0.2.5 (January 6, 2019)

* Fix bug in `UnixDatagram::send` (#782).

# 0.2.4 (November 24, 2018)

* Implement `UnixDatagramFramed`, providing a `Stream + Sink` layer for
  unix domain sockets (#453).
* Add solaris support for `ucred` (#733).
* Documentation tweaks (#754).

# 0.2.3 (October 23, 2018)

* Fix build on NetBSD (#715).

# 0.2.2 (September 27, 2018)

* Fix bug in `UdsStream::read_buf` (#672).

# 0.2.1 (August 19, 2018)

* Re-export `ConnectFuture` (#430).
* bug: Fix `recv_from` (#452).
* bug: Fix build on FreeBSD.

# 0.2.0 (June 6, 2018)

* Initial 0.2 release.
