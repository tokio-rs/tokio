#![warn(rust_2018_idioms)]

pub mod runnable {
    use rustls_dep::internal::pemfile::{certs, pkcs8_private_keys};
    use rustls_dep::{Certificate, NoClientAuth, PrivateKey, ServerConfig};
    use std::fs::File;
    use std::io::{self, BufReader};
    use std::path::Path;
    use std::sync::Arc;
    use tokio;
    use tokio::net::TcpListener;
    use tokio::prelude::*;
    use tokio_tls;
    use tokio_tls::TlsAcceptor;

    fn load_certs(path: &Path) -> io::Result<Vec<Certificate>> {
        certs(&mut BufReader::new(File::open(path)?))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid cert"))
    }

    fn load_keys(path: &Path) -> io::Result<Vec<PrivateKey>> {
        pkcs8_private_keys(&mut BufReader::new(File::open(path)?))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid key"))
    }

    pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Bind the server's socket
        let addr = "127.0.0.1:12345";
        let mut listener = TcpListener::bind(&addr).await?;

        // Create the TLS acceptor.
        let certs = load_certs(Path::new("./examples/domain.crt"))?;
        let mut keys = load_keys(Path::new("./examples/domain.key"))?;

        let mut config = ServerConfig::new(NoClientAuth::new());
        config
            .set_single_cert(certs, keys.remove(0))
            .map_err(|err| {
                println!("Error!!! {:#?}", err);
                io::Error::new(io::ErrorKind::InvalidInput, err)
            })?;

        let tls_acceptor = TlsAcceptor::from(Arc::new(config));

        // Iterate incoming connections
        while let Ok((stream, _)) = listener.accept().await {
            let mut tls_stream = tls_acceptor.accept(stream).await?;

            // Spawn the future as a concurrent task
            tokio::spawn(async move {
                let mut buf = [0; 1024];

                // In a loop, read data from the socket and write the data back.
                loop {
                    let n = tls_stream
                        .read(&mut buf)
                        .await
                        .expect("failed to read data from socket");

                    if n == 0 {
                        return;
                    }

                    tls_stream
                        .write_all(&buf[0..n])
                        .await
                        .expect("failed to write data to socket");
                }
            });
        }

        Ok(())
    }
}
