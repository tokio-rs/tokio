extern crate env_logger;
extern crate futures;
extern crate native_tls;
extern crate tokio;
extern crate tokio_io;
extern crate tokio_tls;

#[macro_use]
extern crate cfg_if;

use std::io::{self, Read, Write};
use std::process::Command;

use futures::stream::Stream;
use futures::{Future, Poll};
use native_tls::{Identity, TlsAcceptor, TlsConnector};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio_io::io::{copy, read_to_end, shutdown};
use tokio_io::{AsyncRead, AsyncWrite};

macro_rules! t {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
        }
    };
}

#[allow(dead_code)]
struct Keys {
    cert_der: Vec<u8>,
    pkey_der: Vec<u8>,
    pkcs12_der: Vec<u8>,
}

#[allow(dead_code)]
fn openssl_keys() -> &'static Keys {
    static INIT: Once = ONCE_INIT;
    static mut KEYS: *mut Keys = 0 as *mut _;

    INIT.call_once(|| {
        let path = t!(env::current_exe());
        let path = path.parent().unwrap();
        let keyfile = path.join("test.key");
        let certfile = path.join("test.crt");
        let config = path.join("openssl.config");

        File::create(&config)
            .unwrap()
            .write_all(
                b"\
            [req]\n\
            distinguished_name=dn\n\
            [ dn ]\n\
            CN=localhost\n\
            [ ext ]\n\
            basicConstraints=CA:FALSE,pathlen:0\n\
            subjectAltName = @alt_names
            [alt_names]
            DNS.1 = localhost
        ",
            )
            .unwrap();

        let subj = "/C=US/ST=Denial/L=Sprintfield/O=Dis/CN=localhost";
        let output = t!(Command::new("openssl")
            .arg("req")
            .arg("-nodes")
            .arg("-x509")
            .arg("-newkey")
            .arg("rsa:2048")
            .arg("-config")
            .arg(&config)
            .arg("-extensions")
            .arg("ext")
            .arg("-subj")
            .arg(subj)
            .arg("-keyout")
            .arg(&keyfile)
            .arg("-out")
            .arg(&certfile)
            .arg("-days")
            .arg("1")
            .output());
        assert!(output.status.success());

        let crtout = t!(Command::new("openssl")
            .arg("x509")
            .arg("-outform")
            .arg("der")
            .arg("-in")
            .arg(&certfile)
            .output());
        assert!(crtout.status.success());
        let keyout = t!(Command::new("openssl")
            .arg("rsa")
            .arg("-outform")
            .arg("der")
            .arg("-in")
            .arg(&keyfile)
            .output());
        assert!(keyout.status.success());

        let pkcs12out = t!(Command::new("openssl")
            .arg("pkcs12")
            .arg("-export")
            .arg("-nodes")
            .arg("-inkey")
            .arg(&keyfile)
            .arg("-in")
            .arg(&certfile)
            .arg("-password")
            .arg("pass:foobar")
            .output());
        assert!(pkcs12out.status.success());

        let keys = Box::new(Keys {
            cert_der: crtout.stdout,
            pkey_der: keyout.stdout,
            pkcs12_der: pkcs12out.stdout,
        });
        unsafe {
            KEYS = Box::into_raw(keys);
        }
    });
    unsafe { &*KEYS }
}

cfg_if! {
    if #[cfg(feature = "rustls")] {
        extern crate webpki;
        extern crate untrusted;

        use std::env;
        use std::fs::File;
        use std::process::Command;
        use std::sync::{ONCE_INIT, Once};

        use untrusted::Input;
        use webpki::trust_anchor_util;

        fn server_cx() -> io::Result<ServerContext> {
            let mut cx = ServerContext::new();

            let (cert, key) = keys();
            cx.config_mut()
              .set_single_cert(vec![cert.to_vec()], key.to_vec());

            Ok(cx)
        }

        fn configure_client(cx: &mut ClientContext) {
            let (cert, _key) = keys();
            let cert = Input::from(cert);
            let anchor = trust_anchor_util::cert_der_as_trust_anchor(cert).unwrap();
            cx.config_mut().root_store.add_trust_anchors(&[anchor]);
        }

        // Like OpenSSL we generate certificates on the fly, but for OSX we
        // also have to put them into a specific keychain. We put both the
        // certificates and the keychain next to our binary.
        //
        // Right now I don't know of a way to programmatically create a
        // self-signed certificate, so we just fork out to the `openssl` binary.
        fn keys() -> (&'static [u8], &'static [u8]) {
            static INIT: Once = ONCE_INIT;
            static mut KEYS: *mut (Vec<u8>, Vec<u8>) = 0 as *mut _;

            INIT.call_once(|| {
                let (key, cert) = openssl_keys();
                let path = t!(env::current_exe());
                let path = path.parent().unwrap();
                let keyfile = path.join("test.key");
                let certfile = path.join("test.crt");
                let config = path.join("openssl.config");

                File::create(&config).unwrap().write_all(b"\
                    [req]\n\
                    distinguished_name=dn\n\
                    [ dn ]\n\
                    CN=localhost\n\
                    [ ext ]\n\
                    basicConstraints=CA:FALSE,pathlen:0\n\
                    subjectAltName = @alt_names
                    [alt_names]
                    DNS.1 = localhost
                ").unwrap();

                let subj = "/C=US/ST=Denial/L=Sprintfield/O=Dis/CN=localhost";
                let output = t!(Command::new("openssl")
                                        .arg("req")
                                        .arg("-nodes")
                                        .arg("-x509")
                                        .arg("-newkey").arg("rsa:2048")
                                        .arg("-config").arg(&config)
                                        .arg("-extensions").arg("ext")
                                        .arg("-subj").arg(subj)
                                        .arg("-keyout").arg(&keyfile)
                                        .arg("-out").arg(&certfile)
                                        .arg("-days").arg("1")
                                        .output());
                assert!(output.status.success());

                let crtout = t!(Command::new("openssl")
                                        .arg("x509")
                                        .arg("-outform").arg("der")
                                        .arg("-in").arg(&certfile)
                                        .output());
                assert!(crtout.status.success());
                let keyout = t!(Command::new("openssl")
                                        .arg("rsa")
                                        .arg("-outform").arg("der")
                                        .arg("-in").arg(&keyfile)
                                        .output());
                assert!(keyout.status.success());

                let cert = crtout.stdout;
                let key = keyout.stdout;
                unsafe {
                    KEYS = Box::into_raw(Box::new((cert, key)));
                }
            });
            unsafe {
                (&(*KEYS).0, &(*KEYS).1)
            }
        }
    } else if #[cfg(any(feature = "force-openssl",
                        all(not(target_os = "macos"),
                            not(target_os = "windows"),
                            not(target_os = "ios"))))] {
        extern crate openssl;

        use std::fs::File;
        use std::env;
        use std::sync::{Once, ONCE_INIT};

        fn contexts() -> (tokio_tls::TlsAcceptor, tokio_tls::TlsConnector) {
            let keys = openssl_keys();

            let pkcs12 = t!(Identity::from_pkcs12(&keys.pkcs12_der, "foobar"));
            let srv = TlsAcceptor::builder(pkcs12);

            let cert = t!(native_tls::Certificate::from_der(&keys.cert_der));

            let mut client = TlsConnector::builder();
            t!(client.add_root_certificate(cert).build());

            (t!(srv.build()).into(), t!(client.build()).into())
        }
    } else if #[cfg(any(target_os = "macos", target_os = "ios"))] {
        extern crate security_framework;

        use std::env;
        use std::fs::File;
        use std::sync::{Once, ONCE_INIT};

        fn contexts() -> (tokio_tls::TlsAcceptor, tokio_tls::TlsConnector) {
            let keys = openssl_keys();

            let pkcs12 = t!(Identity::from_pkcs12(&keys.pkcs12_der, "foobar"));
            let srv = TlsAcceptor::builder(pkcs12);

            let cert = native_tls::Certificate::from_der(&keys.cert_der).unwrap();
            let mut client = TlsConnector::builder();
            client.add_root_certificate(cert);

            (t!(srv.build()).into(), t!(client.build()).into())
        }
    } else {
        extern crate schannel;
        extern crate winapi;

        use std::env;
        use std::fs::File;
        use std::io::Error;
        use std::mem;
        use std::ptr;
        use std::sync::{Once, ONCE_INIT};

        use schannel::cert_context::CertContext;
        use schannel::cert_store::{CertStore, CertAdd, Memory};
        use winapi::shared::basetsd::*;
        use winapi::shared::lmcons::*;
        use winapi::shared::minwindef::*;
        use winapi::shared::ntdef::WCHAR;
        use winapi::um::minwinbase::*;
        use winapi::um::sysinfoapi::*;
        use winapi::um::timezoneapi::*;
        use winapi::um::wincrypt::*;

        const FRIENDLY_NAME: &'static str = "tokio-tls localhost testing cert";

        fn contexts() -> (tokio_tls::TlsAcceptor, tokio_tls::TlsConnector) {
            let cert = localhost_cert();
            let mut store = t!(Memory::new()).into_store();
            t!(store.add_cert(&cert, CertAdd::Always));
            let pkcs12_der = t!(store.export_pkcs12("foobar"));
            let pkcs12 = t!(Identity::from_pkcs12(&pkcs12_der, "foobar"));

            let srv = TlsAcceptor::builder(pkcs12);
            let client = TlsConnector::builder();
            (t!(srv.build()).into(), t!(client.build()).into())
        }

        // ====================================================================
        // Magic!
        //
        // Lots of magic is happening here to wrangle certificates for running
        // these tests on Windows. For more information see the test suite
        // in the schannel-rs crate as this is just coyping that.
        //
        // The general gist of this though is that the only way to add custom
        // trusted certificates is to add it to the system store of trust. To
        // do that we go through the whole rigamarole here to generate a new
        // self-signed certificate and then insert that into the system store.
        //
        // This generates some dialogs, so we print what we're doing sometimes,
        // and otherwise we just manage the ephemeral certificates. Because
        // they're in the system store we always ensure that they're only valid
        // for a small period of time (e.g. 1 day).

        fn localhost_cert() -> CertContext {
            static INIT: Once = ONCE_INIT;
            INIT.call_once(|| {
                for cert in local_root_store().certs() {
                    let name = match cert.friendly_name() {
                        Ok(name) => name,
                        Err(_) => continue,
                    };
                    if name != FRIENDLY_NAME {
                        continue
                    }
                    if !cert.is_time_valid().unwrap() {
                        io::stdout().write_all(br#"

The tokio-tls test suite is about to delete an old copy of one of its
certificates from your root trust store. This certificate was only valid for one
day and it is no longer needed. The host should be "localhost" and the
description should mention "tokio-tls".

        "#).unwrap();
                        cert.delete().unwrap();
                    } else {
                        return
                    }
                }

                install_certificate().unwrap();
            });

            for cert in local_root_store().certs() {
                let name = match cert.friendly_name() {
                    Ok(name) => name,
                    Err(_) => continue,
                };
                if name == FRIENDLY_NAME {
                    return cert
                }
            }

            panic!("couldn't find a cert");
        }

        fn local_root_store() -> CertStore {
            if env::var("CI").is_ok() {
                CertStore::open_local_machine("Root").unwrap()
            } else {
                CertStore::open_current_user("Root").unwrap()
            }
        }

        fn install_certificate() -> io::Result<CertContext> {
            unsafe {
                let mut provider = 0;
                let mut hkey = 0;

                let mut buffer = "tokio-tls test suite".encode_utf16()
                                                         .chain(Some(0))
                                                         .collect::<Vec<_>>();
                let res = CryptAcquireContextW(&mut provider,
                                               buffer.as_ptr(),
                                               ptr::null_mut(),
                                               PROV_RSA_FULL,
                                               CRYPT_MACHINE_KEYSET);
                if res != TRUE {
                    // create a new key container (since it does not exist)
                    let res = CryptAcquireContextW(&mut provider,
                                                   buffer.as_ptr(),
                                                   ptr::null_mut(),
                                                   PROV_RSA_FULL,
                                                   CRYPT_NEWKEYSET | CRYPT_MACHINE_KEYSET);
                    if res != TRUE {
                        return Err(Error::last_os_error())
                    }
                }

                // create a new keypair (RSA-2048)
                let res = CryptGenKey(provider,
                                      AT_SIGNATURE,
                                      0x0800<<16 | CRYPT_EXPORTABLE,
                                      &mut hkey);
                if res != TRUE {
                    return Err(Error::last_os_error());
                }

                // start creating the certificate
                let name = "CN=localhost,O=tokio-tls,OU=tokio-tls,\
                            G=tokio_tls".encode_utf16()
                                          .chain(Some(0))
                                          .collect::<Vec<_>>();
                let mut cname_buffer: [WCHAR; UNLEN as usize + 1] = mem::zeroed();
                let mut cname_len = cname_buffer.len() as DWORD;
                let res = CertStrToNameW(X509_ASN_ENCODING,
                                         name.as_ptr(),
                                         CERT_X500_NAME_STR,
                                         ptr::null_mut(),
                                         cname_buffer.as_mut_ptr() as *mut u8,
                                         &mut cname_len,
                                         ptr::null_mut());
                if res != TRUE {
                    return Err(Error::last_os_error());
                }

                let mut subject_issuer = CERT_NAME_BLOB {
                    cbData: cname_len,
                    pbData: cname_buffer.as_ptr() as *mut u8,
                };
                let mut key_provider = CRYPT_KEY_PROV_INFO {
                    pwszContainerName: buffer.as_mut_ptr(),
                    pwszProvName: ptr::null_mut(),
                    dwProvType: PROV_RSA_FULL,
                    dwFlags: CRYPT_MACHINE_KEYSET,
                    cProvParam: 0,
                    rgProvParam: ptr::null_mut(),
                    dwKeySpec: AT_SIGNATURE,
                };
                let mut sig_algorithm = CRYPT_ALGORITHM_IDENTIFIER {
                    pszObjId: szOID_RSA_SHA256RSA.as_ptr() as *mut _,
                    Parameters: mem::zeroed(),
                };
                let mut expiration_date: SYSTEMTIME = mem::zeroed();
                GetSystemTime(&mut expiration_date);
                let mut file_time: FILETIME = mem::zeroed();
                let res = SystemTimeToFileTime(&mut expiration_date,
                                               &mut file_time);
                if res != TRUE {
                    return Err(Error::last_os_error());
                }
                let mut timestamp: u64 = file_time.dwLowDateTime as u64 |
                                         (file_time.dwHighDateTime as u64) << 32;
                // one day, timestamp unit is in 100 nanosecond intervals
                timestamp += (1E9 as u64) / 100 * (60 * 60 * 24);
                file_time.dwLowDateTime = timestamp as u32;
                file_time.dwHighDateTime = (timestamp >> 32) as u32;
                let res = FileTimeToSystemTime(&file_time,
                                               &mut expiration_date);
                if res != TRUE {
                    return Err(Error::last_os_error());
                }

                // create a self signed certificate
                let cert_context = CertCreateSelfSignCertificate(
                        0 as ULONG_PTR,
                        &mut subject_issuer,
                        0,
                        &mut key_provider,
                        &mut sig_algorithm,
                        ptr::null_mut(),
                        &mut expiration_date,
                        ptr::null_mut());
                if cert_context.is_null() {
                    return Err(Error::last_os_error());
                }

                // TODO: this is.. a terrible hack. Right now `schannel`
                //       doesn't provide a public method to go from a raw
                //       cert context pointer to the `CertContext` structure it
                //       has, so we just fake it here with a transmute. This'll
                //       probably break at some point, but hopefully by then
                //       it'll have a method to do this!
                struct MyCertContext<T>(T);
                impl<T> Drop for MyCertContext<T> {
                    fn drop(&mut self) {}
                }

                let cert_context = MyCertContext(cert_context);
                let cert_context: CertContext = mem::transmute(cert_context);

                cert_context.set_friendly_name(FRIENDLY_NAME)?;

                // install the certificate to the machine's local store
                io::stdout().write_all(br#"

The tokio-tls test suite is about to add a certificate to your set of root
and trusted certificates. This certificate should be for the domain "localhost"
with the description related to "tokio-tls". This certificate is only valid
for one day and will be automatically deleted if you re-run the tokio-tls
test suite later.

        "#).unwrap();
                local_root_store().add_cert(&cert_context,
                                                 CertAdd::ReplaceExisting)?;
                Ok(cert_context)
            }
        }
    }
}

fn native2io(e: native_tls::Error) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e)
}

const AMT: u64 = 128 * 1024;

#[test]
fn client_to_server() {
    drop(env_logger::try_init());
    let mut l = t!(Runtime::new());

    // Create a server listening on a port, then figure out what that port is
    let srv = t!(TcpListener::bind(&t!("127.0.0.1:0".parse())));
    let addr = t!(srv.local_addr());

    let (server_cx, client_cx) = contexts();

    // Create a future to accept one socket, connect the ssl stream, and then
    // read all the data from it.
    let socket = srv.incoming().take(1).collect();
    let received = socket
        .map(|mut socket| socket.remove(0))
        .and_then(move |socket| server_cx.accept(socket).map_err(native2io))
        .and_then(|socket| read_to_end(socket, Vec::new()));

    // Create a future to connect to our server, connect the ssl stream, and
    // then write a bunch of data to it.
    let client = TcpStream::connect(&addr);
    let sent = client
        .and_then(move |socket| client_cx.connect("localhost", socket).map_err(native2io))
        .and_then(|socket| copy(io::repeat(9).take(AMT), socket))
        .and_then(|(amt, _repeat, socket)| shutdown(socket).map(move |_| amt));

    // Finally, run everything!
    let (amt, (_, data)) = t!(l.block_on(sent.join(received)));
    assert_eq!(amt, AMT);
    assert!(data == vec![9; amt as usize]);
}

#[test]
fn server_to_client() {
    drop(env_logger::try_init());
    let mut l = t!(Runtime::new());

    // Create a server listening on a port, then figure out what that port is
    let srv = t!(TcpListener::bind(&t!("127.0.0.1:0".parse())));
    let addr = t!(srv.local_addr());

    let (server_cx, client_cx) = contexts();

    let socket = srv.incoming().take(1).collect();
    let sent = socket
        .map(|mut socket| socket.remove(0))
        .and_then(move |socket| server_cx.accept(socket).map_err(native2io))
        .and_then(|socket| copy(io::repeat(9).take(AMT), socket))
        .and_then(|(amt, _repeat, socket)| shutdown(socket).map(move |_| amt));

    let client = TcpStream::connect(&addr);
    let received = client
        .and_then(move |socket| client_cx.connect("localhost", socket).map_err(native2io))
        .and_then(|socket| read_to_end(socket, Vec::new()));

    // Finally, run everything!
    let (amt, (_, data)) = t!(l.block_on(sent.join(received)));
    assert_eq!(amt, AMT);
    assert!(data == vec![9; amt as usize]);
}

struct OneByte<S> {
    inner: S,
}

impl<S: Read> Read for OneByte<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(&mut buf[..1])
    }
}

impl<S: Write> Write for OneByte<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(&buf[..1])
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<S: AsyncRead> AsyncRead for OneByte<S> {}
impl<S: AsyncWrite> AsyncWrite for OneByte<S> {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        self.inner.shutdown()
    }
}

#[test]
fn one_byte_at_a_time() {
    const AMT: u64 = 1024;
    drop(env_logger::try_init());
    let mut l = t!(Runtime::new());

    let srv = t!(TcpListener::bind(&t!("127.0.0.1:0".parse())));
    let addr = t!(srv.local_addr());

    let (server_cx, client_cx) = contexts();

    let socket = srv.incoming().take(1).collect();
    let sent = socket
        .map(|mut socket| socket.remove(0))
        .and_then(move |socket| {
            server_cx
                .accept(OneByte { inner: socket })
                .map_err(native2io)
        })
        .and_then(|socket| copy(io::repeat(9).take(AMT), socket))
        .and_then(|(amt, _repeat, socket)| shutdown(socket).map(move |_| amt));

    let client = TcpStream::connect(&addr);
    let received = client
        .and_then(move |socket| {
            let socket = OneByte { inner: socket };
            client_cx.connect("localhost", socket).map_err(native2io)
        })
        .and_then(|socket| read_to_end(socket, Vec::new()));

    let (amt, (_, data)) = t!(l.block_on(sent.join(received)));
    assert_eq!(amt, AMT);
    assert!(data == vec![9; amt as usize]);
}
