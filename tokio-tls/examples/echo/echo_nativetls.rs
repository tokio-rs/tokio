#![warn(rust_2018_idioms)]

pub mod runnable {
    use native_tls;
    use native_tls::Identity;
    use tokio;
    use tokio::net::TcpListener;
    use tokio::prelude::*;
    use tokio_tls;

    pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Bind the server's socket
        let addr = "127.0.0.1:12345";
        let mut listener = TcpListener::bind(&addr).await?;

        // Create the TLS acceptor.
        let der = include_bytes!("identity.p12");
        let cert = Identity::from_pkcs12(der, "mypass")?;
        let tls_acceptor =
            tokio_tls::TlsAcceptor::from(native_tls::TlsAcceptor::builder(cert).build()?);

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
