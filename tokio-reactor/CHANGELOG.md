# 0.1.6 (September 27, 2018)

* Fix panic when reactor is stored in a thread-local (#628).

# 0.1.5 (August 27, 2018)

* Experimental async / await support.

# 0.1.4 (August 23, 2018)

* Use a scalable RW lock (#517)
* Implement std::error::Error for error types (#511)
* Documentation improvements

# 0.1.3 (August 6, 2018)

* Misc small fixes (#508)

# 0.1.2 (June 13, 2018)

* Fix deadlock that can happen when shutting down (#409)
* Handle::default() lazily binds to reactor (#350)

# 0.1.1 (March 22, 2018)

* Fix threading bugs (#227)
* Fix notification bugs (#243)
* Optionally support futures 0.2 (#172)

# 0.1.0 (March 09, 2018)

* Initial release
