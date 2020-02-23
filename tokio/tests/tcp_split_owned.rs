#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::io::Result;
use std::io::{Read, Write};
use std::{net, thread};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::test]
async fn split() -> Result<()> {
    const MSG: &[u8] = b"split";

    let listener = net::TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.write(MSG).unwrap();

        let mut read_buf = [0u8; 32];
        let read_len = stream.read(&mut read_buf).unwrap();
        assert_eq!(&read_buf[..read_len], MSG);
    });

    let stream = TcpStream::connect(&addr).await?;
    let (mut read_half, mut write_half) = stream.split_owned();

    let write_join = tokio::spawn(async move {
        write_half.write(MSG).await
    });

    let mut read_buf = [0u8; 32];
    let peek_len1 = read_half.peek(&mut read_buf[..]).await?;
    let peek_len2 = read_half.peek(&mut read_buf[..]).await?;
    assert_eq!(peek_len1, peek_len2);

    let read_len = read_half.read(&mut read_buf[..]).await?;
    assert_eq!(peek_len1, read_len);
    assert_eq!(&read_buf[..read_len], MSG);

    write_join.await.unwrap().unwrap();
    handle.join().unwrap();
    Ok(())
}

#[tokio::test]
async fn reunite() -> Result<()> {
    let listener = net::TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let handle = thread::spawn(move || {
        drop(listener.accept().unwrap());
        drop(listener.accept().unwrap());
    });

    let stream1 = TcpStream::connect(&addr).await?;
    let (read1, write1) = stream1.split_owned();

    let stream2 = TcpStream::connect(&addr).await?;
    let (_,     write2) = stream2.split_owned();

    let read1 = match read1.reunite(write2) {
        Ok(_) => panic!("Reunite should not succeed"),
        Err(err) => err.0,
    };

    read1.reunite(write1).expect("Reunite should succeed");

    handle.join().unwrap();
    Ok(())
}
