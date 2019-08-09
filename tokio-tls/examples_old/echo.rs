#![warn(rust_2018_idioms)]

// A tiny async TLS echo server with Tokio
use native_tls;
use native_tls::Identity;
use tokio;
use tokio::io;
use tokio::net::TcpListener;
use tokio::prelude::*;
use tokio_tls;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Bind the server's socket
    let addr = "127.0.0.1:12345".parse()?;
    let tcp = TcpListener::bind(&addr)?;

    // Create the TLS acceptor.
    let der = include_bytes!("identity.p12");
    let cert = Identity::from_pkcs12(der, "mypass")?;
    let tls_acceptor =
        tokio_tls::TlsAcceptor::from(native_tls::TlsAcceptor::builder(cert).build()?);

    // Iterate incoming connections
    let server = tcp
        .incoming()
        .for_each(move |tcp| {
            // Accept the TLS connection.
            let tls_accept = tls_acceptor
                .accept(tcp)
                .and_then(move |tls| {
                    // Split up the read and write halves
                    let (reader, writer) = tls.split();

                    // Copy the data back to the client
                    let conn = io::copy(reader, writer)
                        // print what happened
                        .map(|(n, _, _)| println!("wrote {} bytes", n))
                        // Handle any errors
                        .map_err(|err| println!("IO error {:?}", err));

                    // Spawn the future as a concurrent task
                    tokio::spawn(conn);

                    Ok(())
                })
                .map_err(|err| {
                    println!("TLS accept error: {:?}", err);
                });
            tokio::spawn(tls_accept);

            Ok(())
        })
        .map_err(|err| {
            println!("server error {:?}", err);
        });

    // Start the runtime and spin up the server
    tokio::run(server);
    Ok(())
}
