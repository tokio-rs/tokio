extern crate env_logger;
extern crate futures;
extern crate native_tls;
extern crate tokio;
extern crate tokio_io;
extern crate tokio_tls;

#[macro_use]
extern crate cfg_if;

use std::io;
use std::net::ToSocketAddrs;

use futures::Future;
use native_tls::TlsConnector;
use tokio::net::TcpStream;
use tokio::runtime::Runtime;
use tokio_io::io::{flush, read_to_end, write_all};

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
        fn assert_bad_hostname_error(err: &io::Error) {
            let err = err.to_string();
            assert!(err.contains("CertNotValidForName"), "bad error: {}", err);
        }
    } else if #[cfg(any(feature = "force-openssl",
                        all(not(target_os = "macos"),
                            not(target_os = "windows"),
                            not(target_os = "ios"))))] {
        extern crate openssl;

        fn assert_bad_hostname_error(err: &io::Error) {
            let err = err.get_ref().unwrap();
            let err = err.downcast_ref::<native_tls::Error>().unwrap();
            assert!(format!("{}", err).contains("certificate verify failed"));
        }
    } else if #[cfg(any(target_os = "macos", target_os = "ios"))] {
        fn assert_bad_hostname_error(err: &io::Error) {
            let err = err.get_ref().unwrap();
            let err = err.downcast_ref::<native_tls::Error>().unwrap();
            assert!(format!("{}", err).contains("was not trusted."));
        }
    } else {
        fn assert_bad_hostname_error(err: &io::Error) {
            let err = err.get_ref().unwrap();
            let err = err.downcast_ref::<native_tls::Error>().unwrap();
            assert!(format!("{}", err).contains("CN name"));
        }
    }
}

fn native2io(e: native_tls::Error) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e)
}

#[test]
fn fetch_google() {
    drop(env_logger::try_init());

    // First up, resolve google.com
    let addr = t!("google.com:443".to_socket_addrs()).next().unwrap();

    // Create an event loop and connect a socket to our resolved address.c
    let mut l = t!(Runtime::new());
    let client = TcpStream::connect(&addr);

    // Send off the request by first negotiating an SSL handshake, then writing
    // of our request, then flushing, then finally read off the response.
    let data = client
        .and_then(move |socket| {
            let builder = TlsConnector::builder();
            let connector = t!(builder.build());
            let connector = tokio_tls::TlsConnector::from(connector);
            connector.connect("google.com", socket).map_err(native2io)
        })
        .and_then(|socket| write_all(socket, b"GET / HTTP/1.0\r\n\r\n"))
        .and_then(|(socket, _)| flush(socket))
        .and_then(|socket| read_to_end(socket, Vec::new()));

    let (_, data) = t!(l.block_on(data));

    // any response code is fine
    assert!(data.starts_with(b"HTTP/1.0 "));

    let data = String::from_utf8_lossy(&data);
    let data = data.trim_right();
    assert!(data.ends_with("</html>") || data.ends_with("</HTML>"));
}

// see comment in bad.rs for ignore reason
#[cfg_attr(all(target_os = "macos", feature = "force-openssl"), ignore)]
#[test]
fn wrong_hostname_error() {
    drop(env_logger::try_init());

    let addr = t!("google.com:443".to_socket_addrs()).next().unwrap();

    let mut l = t!(Runtime::new());
    let client = TcpStream::connect(&addr);
    let data = client.and_then(move |socket| {
        let builder = TlsConnector::builder();
        let connector = t!(builder.build());
        let connector = tokio_tls::TlsConnector::from(connector);
        connector
            .connect("rust-lang.org", socket)
            .map_err(native2io)
    });

    let res = l.block_on(data);
    assert!(res.is_err());
    assert_bad_hostname_error(&res.err().unwrap());
}
