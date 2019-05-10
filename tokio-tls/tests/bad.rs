#![deny(warnings, rust_2018_idioms)]

use cfg_if::cfg_if;
use env_logger;
use futures::Future;
use native_tls::TlsConnector;
use std::io::{self, Error};
use std::net::ToSocketAddrs;
use tokio::net::TcpStream;
use tokio::runtime::Runtime;
use tokio_tls;

macro_rules! t {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
        }
    };
}

cfg_if! {
    if #[cfg(feature = "force-rustls")] {
        fn verify_failed(err: &Error, s:  &str) {
            let err = err.to_string();
            assert!(err.contains(s), "bad error: {}", err);
        }

        fn assert_expired_error(err: &Error) {
            verify_failed(err, "CertExpired");
        }

        fn assert_wrong_host(err: &Error) {
            verify_failed(err, "CertNotValidForName");
        }

        fn assert_self_signed(err: &Error) {
            verify_failed(err, "UnknownIssuer");
        }

        fn assert_untrusted_root(err: &Error) {
            verify_failed(err, "UnknownIssuer");
        }
    } else if #[cfg(any(feature = "force-openssl",
                        all(not(target_os = "macos"),
                            not(target_os = "windows"),
                            not(target_os = "ios"))))] {
        fn verify_failed(err: &Error) {
            assert!(format!("{}", err).contains("certificate verify failed"))
        }

        use verify_failed as assert_expired_error;
        use verify_failed as assert_wrong_host;
        use verify_failed as assert_self_signed;
        use verify_failed as assert_untrusted_root;
    } else if #[cfg(any(target_os = "macos", target_os = "ios"))] {

        fn assert_invalid_cert_chain(err: &Error) {
            assert!(format!("{}", err).contains("was not trusted."))
        }

        use crate::assert_invalid_cert_chain as assert_expired_error;
        use crate::assert_invalid_cert_chain as assert_wrong_host;
        use crate::assert_invalid_cert_chain as assert_self_signed;
        use crate::assert_invalid_cert_chain as assert_untrusted_root;
    } else {
        fn assert_expired_error(err: &Error) {
            let s = err.to_string();
            assert!(s.contains("system clock"), "error = {:?}", s);
        }

        fn assert_wrong_host(err: &Error) {
            let s = err.to_string();
            assert!(s.contains("CN name"), "error = {:?}", s);
        }

        fn assert_self_signed(err: &Error) {
            let s = err.to_string();
            assert!(s.contains("root certificate which is not trusted"), "error = {:?}", s);
        }

        use assert_self_signed as assert_untrusted_root;
    }
}

fn get_host(host: &'static str) -> Error {
    drop(env_logger::try_init());

    let addr = format!("{}:443", host);
    let addr = t!(addr.to_socket_addrs()).next().unwrap();

    let mut l = t!(Runtime::new());
    let client = TcpStream::connect(&addr);
    let data = client.and_then(move |socket| {
        let builder = TlsConnector::builder();
        let cx = builder.build().unwrap();
        let cx = tokio_tls::TlsConnector::from(cx);
        cx.connect(host, socket)
            .map_err(|e| Error::new(io::ErrorKind::Other, e))
    });

    let res = l.block_on(data);
    assert!(res.is_err());
    res.err().unwrap()
}

#[test]
fn expired() {
    assert_expired_error(&get_host("expired.badssl.com"))
}

// TODO: the OSX builders on Travis apparently fail this tests spuriously?
//       passes locally though? Seems... bad!
#[test]
#[cfg_attr(all(target_os = "macos", feature = "force-openssl"), ignore)]
fn wrong_host() {
    assert_wrong_host(&get_host("wrong.host.badssl.com"))
}

#[test]
fn self_signed() {
    assert_self_signed(&get_host("self-signed.badssl.com"))
}

#[test]
fn untrusted_root() {
    assert_untrusted_root(&get_host("untrusted-root.badssl.com"))
}
