#![doc(html_root_url = "https://docs.rs/tokio-tls/0.3.0")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(intra_doc_link_resolution_failure)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]

//! Async TLS streams
//!
//! This library is an implementation of TLS streams using the most appropriate
//! system library by default for negotiating the connection. That is, on
//! Windows this library uses SChannel, on OSX it uses SecureTransport, and on
//! other platforms it uses OpenSSL.
//!
//! Each TLS stream implements the `Read` and `Write` traits to interact and
//! interoperate with the rest of the futures I/O ecosystem. Client connections
//! initiated from this crate verify hostnames automatically and by default.
//!
//! This crate primarily exports this ability through two newtypes,
//! `TlsConnector` and `TlsAcceptor`. These newtypes augment the
//! functionality provided by the `native-tls` crate, on which this crate is
//! built. Configuration of TLS parameters is still primarily done through the
//! `native-tls` crate.
//!
//! This crate also has the option to rely on rustls. It provides the same,
//! albeit shimmed, interface as native-tls for easy interoperability.

#[cfg(feature = "native-tls")]
mod lib_nativetls;

#[cfg(feature = "native-tls")]
pub use lib_nativetls::*;

// native-tls takes precedence in order to maintain backwards compat
#[cfg(all(feature = "rustls", not(feature = "native-tls")))]
mod lib_rustls;

#[cfg(all(feature = "rustls", not(feature = "native-tls")))]
pub use lib_rustls::*;
