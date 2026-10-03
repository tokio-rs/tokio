#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::io::ErrorKind;
use tokio::io;

mod support {
    pub mod io_coop;
}
use support::io_coop::{ByteAtATimeReader, ByteAtATimeWriter};

#[tokio::test]
async fn retry_on_io_interrupted() {
    let mut reader = tokio_test::io::Builder::new()
        .read_error(ErrorKind::Interrupted.into())
        .read(b"ab")
        .read_error(ErrorKind::Interrupted.into())
        .read(b"cd")
        .build();
    let mut writer = tokio_test::io::Builder::new()
        .write_error(ErrorKind::Interrupted.into())
        .write(b"a")
        .write_error(ErrorKind::Interrupted.into())
        .write(b"bcd")
        .build();
    let count = tokio::io::copy_buf(
        &mut tokio::io::BufReader::with_capacity(2, &mut reader),
        &mut writer,
    )
    .await;
    assert_eq!(count.unwrap(), 4);
}

#[tokio::test]
async fn always_ready_reads_are_cooperative() {
    let expected = b"abcd".repeat(64);
    let mut reader = ByteAtATimeReader {
        data: &expected,
        interruptions_remaining: 0,
    };
    let mut output = Vec::new();
    let mut copy = tokio_test::task::spawn(io::copy_buf(&mut reader, &mut output));

    tokio_test::assert_pending!(copy.poll());

    assert_eq!(copy.await.unwrap(), expected.len() as u64);
    assert_eq!(output, expected);
}

#[tokio::test]
async fn always_ready_writes_are_cooperative() {
    // Successful one-byte writes must yield without any Interrupted errors.
    let expected = b"abcd".repeat(64);
    let mut reader = &expected[..];
    let mut writer = ByteAtATimeWriter {
        data: Vec::new(),
        interruptions_remaining: 0,
    };
    let mut copy = tokio_test::task::spawn(io::copy_buf(&mut reader, &mut writer));

    tokio_test::assert_pending!(copy.poll());
    assert_eq!(copy.await.unwrap(), expected.len() as u64);
    assert_eq!(writer.data, expected);
}

#[tokio::test]
async fn interrupted_reads_are_cooperative() {
    let expected = b"abcd";
    let mut reader = ByteAtATimeReader {
        data: expected,
        interruptions_remaining: 256,
    };
    let mut output = Vec::new();
    let mut copy = tokio_test::task::spawn(io::copy_buf(&mut reader, &mut output));

    tokio_test::assert_pending!(copy.poll());
    assert_eq!(copy.await.unwrap(), expected.len() as u64);
    assert_eq!(output, expected);
}

#[tokio::test]
async fn interrupted_writes_are_cooperative() {
    let expected = b"abcd";
    let mut reader = &expected[..];
    let mut writer = ByteAtATimeWriter {
        data: Vec::new(),
        interruptions_remaining: 256,
    };
    let mut copy = tokio_test::task::spawn(io::copy_buf(&mut reader, &mut writer));

    tokio_test::assert_pending!(copy.poll());
    assert_eq!(copy.await.unwrap(), expected.len() as u64);
    assert_eq!(writer.data, expected);
}

#[tokio::test]
async fn interrupted_reads_remain_unconstrained() {
    let expected = b"abcd".repeat(64);
    let mut reader = ByteAtATimeReader {
        data: &expected,
        interruptions_remaining: 256,
    };
    let mut output = Vec::new();
    // disabling the budget lets the same input finish in a single poll
    let bytes_copied = {
        let copy = tokio::task::unconstrained(io::copy_buf(&mut reader, &mut output));
        let mut copy = tokio_test::task::spawn(copy);
        tokio_test::assert_ready_ok!(copy.poll())
    };

    assert_eq!(bytes_copied, expected.len() as u64);
    assert_eq!(output, expected);
}

#[tokio::test]
async fn interrupted_writes_remain_unconstrained() {
    let expected = b"abcd".repeat(64);
    let mut reader = &expected[..];
    let mut writer = ByteAtATimeWriter {
        data: Vec::new(),
        interruptions_remaining: 256,
    };
    // disabling the budget lets Interrupted retries and writes finish in a single poll
    let bytes_copied = {
        let copy = tokio::task::unconstrained(io::copy_buf(&mut reader, &mut writer));
        let mut copy = tokio_test::task::spawn(copy);
        tokio_test::assert_ready_ok!(copy.poll())
    };

    assert_eq!(bytes_copied, expected.len() as u64);
    assert_eq!(writer.data, expected);
}
