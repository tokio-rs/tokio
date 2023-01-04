#![cfg(feature = "full")]
#![cfg(unix)]

use std::convert::TryFrom;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::pipe;

use tokio::process::Command;
use tokio_test::task;
use tokio_test::{assert_err, assert_ok, assert_pending, assert_ready_ok};

/// Helper struct which will clean up temporary files once the `TempFifo` is dropped.
struct TempFifo {
    path: PathBuf,
    _dir: tempfile::TempDir,
}

impl TempFifo {
    fn new(name: &str) -> io::Result<TempFifo> {
        let dir = tempfile::Builder::new()
            .prefix("tokio-fifo-tests")
            .tempdir()?;
        let path = dir.path().join(name);
        nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRWXU)?;

        Ok(TempFifo { path, _dir: dir })
    }
}

impl AsRef<Path> for TempFifo {
    fn as_ref(&self) -> &Path {
        self.path.as_ref()
    }
}

#[tokio::test]
async fn fifo_simple_send() -> io::Result<()> {
    let fifo = TempFifo::new("simple_send")?;
    const DATA: &[u8] = b"this is some data to write to the fifo";

    let mut reader = pipe::Receiver::open(&fifo)?;
    let mut read_fut = task::spawn(async move {
        let mut buf = vec![0; DATA.len()];
        reader.read_exact(&mut buf).await?;
        Ok::<_, io::Error>(buf)
    });
    assert_pending!(read_fut.poll());

    let mut writer = pipe::Sender::open(&fifo)?;
    writer.write_all(DATA).await?;

    // Let the IO driver poll events for the reader.
    while !read_fut.is_woken() {
        tokio::task::yield_now().await;
    }

    let read_data = assert_ready_ok!(read_fut.poll());
    assert_eq!(&read_data, DATA);

    Ok(())
}

#[tokio::test]
#[cfg(target_os = "linux")]
async fn fifo_simple_send_sender_first() -> io::Result<()> {
    let fifo = TempFifo::new("simple_send_sender_first")?;
    const DATA: &[u8] = b"this is some data to write to the fifo";

    // Simple open should fail with ENXIO (no such device or address).
    let err = assert_err!(pipe::Sender::open(&fifo));
    assert_eq!(err.raw_os_error(), Some(libc::ENXIO));

    let mut writer = pipe::Sender::open_dangling(&fifo)?;
    writer.write_all(DATA).await?;

    let mut reader = pipe::Receiver::open(&fifo)?;
    let mut read_data = vec![0; DATA.len()];
    reader.read_exact(&mut read_data).await?;
    assert_eq!(&read_data, DATA);

    Ok(())
}

#[tokio::test]
async fn detects_not_a_fifo() -> io::Result<()> {
    let dir = tempfile::Builder::new()
        .prefix("tokio-fifo-tests")
        .tempdir()
        .unwrap();
    let path = dir.path().join("not_a_fifo");
    File::create(&path)?;

    let err = assert_err!(pipe::Sender::open(&path));
    assert_eq!(err.kind(), io::ErrorKind::Other);

    let err = assert_err!(pipe::Receiver::open(&path));
    assert_eq!(err.kind(), io::ErrorKind::Other);

    Ok(())
}

#[tokio::test]
async fn from_child_stdout() -> io::Result<()> {
    const MSG: &[u8] = b"hello_world";

    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(format!("echo -n {}", std::str::from_utf8(MSG).unwrap()))
        .stdout(Stdio::piped());
    let mut handle = cmd.spawn()?;

    let mut reader = pipe::Receiver::try_from(handle.stdout.take().unwrap())?;
    let mut read_data = vec![0; MSG.len()];
    reader.read_exact(&mut read_data).await?;
    assert_eq!(&read_data, MSG);

    let status = assert_ok!(handle.wait().await);
    assert_eq!(status.code(), Some(0));

    Ok(())
}

#[tokio::test]
async fn from_child_stdin() -> io::Result<()> {
    const MSG: &[u8] = b"hello_world";

    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(format!(
            r#"x=$(cat); [ "$x" = "{}" ]"#,
            std::str::from_utf8(MSG).unwrap()
        ))
        .stdin(Stdio::piped());
    let mut handle = cmd.spawn()?;

    let mut writer = pipe::Sender::try_from(handle.stdin.take().unwrap())?;
    writer.write_all(MSG).await?;
    drop(writer);

    let status = assert_ok!(handle.wait().await);
    assert_eq!(status.code(), Some(0));

    Ok(())
}

fn writable_by_poll(writer: &pipe::Sender) -> bool {
    task::spawn(writer.writable()).poll().is_ready()
}

#[tokio::test]
async fn try_read_write() -> io::Result<()> {
    const DATA: &[u8] = b"this is some data to write to the fifo";

    // Create a pair
    let fifo = TempFifo::new("try_read_write")?;
    let reader = pipe::Receiver::open(&fifo)?;
    let writer = pipe::Sender::open(&fifo)?;

    // Fill the pipe buffer
    let mut write_data = Vec::new();
    while writable_by_poll(&writer) {
        match writer.try_write(DATA) {
            Ok(n) => write_data.extend(&DATA[..n]),
            Err(e) => {
                assert_eq!(e.kind(), io::ErrorKind::WouldBlock);
                break;
            }
        }
    }

    // Drain the pipe buffer
    let mut read_data = vec![0; write_data.len()];
    let mut i = 0;
    while i < write_data.len() {
        reader.readable().await?;
        match reader.try_read(&mut read_data[i..]) {
            Ok(n) => i += n,
            Err(e) => {
                assert_eq!(e.kind(), io::ErrorKind::WouldBlock);
                continue;
            }
        }
    }

    assert_eq!(read_data, write_data);

    Ok(())
}

#[tokio::test]
async fn try_read_write_vectored() -> io::Result<()> {
    const DATA: &[u8] = b"this is some data to write to the fifo";

    // Create a pair
    let fifo = TempFifo::new("try_read_write_vectored")?;
    let reader = pipe::Receiver::open(&fifo)?;
    let writer = pipe::Sender::open(&fifo)?;

    let write_bufs: Vec<_> = DATA.chunks(3).map(io::IoSlice::new).collect();

    // Fill the pipe buffer
    let mut write_data = Vec::new();
    while writable_by_poll(&writer) {
        match writer.try_write_vectored(&write_bufs) {
            Ok(n) => write_data.extend(&DATA[..n]),
            Err(e) => {
                assert_eq!(e.kind(), io::ErrorKind::WouldBlock);
                break;
            }
        }
    }

    // Drain the pipe buffer
    let mut read_data = vec![0; write_data.len()];
    let mut i = 0;
    while i < write_data.len() {
        reader.readable().await?;

        let mut read_bufs: Vec<_> = read_data[i..]
            .chunks_mut(0x10000)
            .map(io::IoSliceMut::new)
            .collect();
        match reader.try_read_vectored(&mut read_bufs) {
            Ok(n) => i += n,
            Err(e) => {
                assert_eq!(e.kind(), io::ErrorKind::WouldBlock);
                continue;
            }
        }
    }

    assert_eq!(read_data, write_data);

    Ok(())
}
