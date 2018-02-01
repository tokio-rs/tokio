# 0.1.4 (November 10, 2017)

* Use `FrameTooBig` as length delimited error type (#70).
* Provide `Bytes` and `Lines` codecs (#78).
* Provide `AllowStdIo` wrapper (#76).

# 0.1.3 (August 14, 2017)

* Fix bug involving zero sized writes in copy helper (#57).
* Add get / set accessors for length delimited max frame length setting. (#65).
* Add `Framed::into_parts_and_codec` (#59).

# 0.1.2 (May 23, 2017)

* Add `from_parts` and `into_parts` to the framing combinators.
* Support passing an initialized buffer to the framing combinators.
* Add `length_adjustment` support to length delimited encoding (#48).

# 0.1.1 (March 22, 2017)

* Add some omitted `Self: Sized` bounds.
* Add missing "inner" fns.

# 0.1.0 (March 15, 2017)

* Initial release
