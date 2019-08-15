#![feature(async_await)]

use tokio_net::tcp::{TcpListener, TcpStream};

#[tokio::test]
async fn split_reunite() -> std::io::Result<()> {
    let listener = TcpListener::bind(&"127.0.0.1:0".parse().unwrap())?;
    let stream = TcpStream::connect(&listener.local_addr()?).await?;

    let (r, w) = stream.split();
    assert!(r.reunite(w).is_ok());
    Ok(())
}

#[tokio::test]
async fn split_reunite_error() -> std::io::Result<()> {
    let listener = TcpListener::bind(&"127.0.0.1:0".parse().unwrap())?;
    let stream = TcpStream::connect(&listener.local_addr()?).await?;
    let stream1 = TcpStream::connect(&listener.local_addr()?).await?;

    let (r, _) = stream.split();
    let (_, w) = stream1.split();
    assert!(r.reunite(w).is_err());
    Ok(())
}
