# 0.1.5 (August 30, 2019)

* Allow `UdpFramed::new` to revert to previous behavior for decoding frames (#1517)
* Fix `UdpFramed` decoding to repeatedly call `Decoder::decode_eof (#1517)

# 0.1.4 (August 28, 2019)

* Fix `UdpFramed`'s ability to decode multiple frames in one datagram (#1444)

# 0.1.3 (November 21, 2018)

* Add `RecvDgram::into_parts` (#710)

# 0.1.2 (August 23, 2018)

* Provide methods to inspect readiness (#522)

# 0.1.1 (June 13, 2018)

* Switch to tokio-codec (#360)

# 0.1.0 (Mar 23, 2018)

* Initial release
