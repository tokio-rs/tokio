#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi")))] // WASI does not support all fs operations

use futures::future::FutureExt;
use std::io::IoSlice;
use tempfile::NamedTempFile;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};
use tokio_test::task;

const HELLO: &[u8] = b"hello world...";

#[tokio::test]
async fn basic_read() {
    let mut tempfile = tempfile();
    tempfile.write_all(HELLO).unwrap();

    let mut file = File::open(tempfile.path()).await.unwrap();

    let mut buf = [0; 1024];
    let n = file.read(&mut buf).await.unwrap();

    assert_eq!(n, HELLO.len());
    assert_eq!(&buf[..n], HELLO);
}

#[tokio::test]
async fn basic_write() {
    let tempfile = tempfile();

    let mut file = File::create(tempfile.path()).await.unwrap();

    file.write_all(HELLO).await.unwrap();
    file.flush().await.unwrap();

    let file = std::fs::read(tempfile.path()).unwrap();
    assert_eq!(file, HELLO);
}

#[tokio::test]
async fn basic_write_and_shutdown() {
    let tempfile = tempfile();

    let mut file = File::create(tempfile.path()).await.unwrap();

    file.write_all(HELLO).await.unwrap();
    file.shutdown().await.unwrap();

    let file = std::fs::read(tempfile.path()).unwrap();
    assert_eq!(file, HELLO);
}

#[tokio::test]
async fn write_vectored() {
    let tempfile = tempfile();

    let mut file = File::create(tempfile.path()).await.unwrap();

    let ret = file
        .write_vectored(&[IoSlice::new(HELLO), IoSlice::new(HELLO)])
        .await
        .unwrap();
    assert_eq!(ret, HELLO.len() * 2);
    file.flush().await.unwrap();

    let file = std::fs::read(tempfile.path()).unwrap();
    assert_eq!(file, [HELLO, HELLO].concat());
}

#[tokio::test]
async fn write_vectored_and_shutdown() {
    let tempfile = tempfile();

    let mut file = File::create(tempfile.path()).await.unwrap();

    let ret = file
        .write_vectored(&[IoSlice::new(HELLO), IoSlice::new(HELLO)])
        .await
        .unwrap();
    assert_eq!(ret, HELLO.len() * 2);
    file.shutdown().await.unwrap();

    let file = std::fs::read(tempfile.path()).unwrap();
    assert_eq!(file, [HELLO, HELLO].concat());
}

#[tokio::test]
async fn rewind_seek_position() {
    let tempfile = tempfile();

    let mut file = File::create(tempfile.path()).await.unwrap();

    file.seek(SeekFrom::Current(10)).await.unwrap();

    file.rewind().await.unwrap();

    assert_eq!(file.stream_position().await.unwrap(), 0);
}

#[tokio::test]
async fn coop() {
    let mut tempfile = tempfile();
    tempfile.write_all(HELLO).unwrap();

    let mut task = task::spawn(async {
        let mut file = File::open(tempfile.path()).await.unwrap();

        let mut buf = [0; 1024];

        loop {
            let _ = file.read(&mut buf).await.unwrap();
            file.seek(std::io::SeekFrom::Start(0)).await.unwrap();
        }
    });

    for _ in 0..1_000 {
        if task.poll().is_pending() {
            return;
        }
    }

    panic!("did not yield");
}

#[tokio::test]
async fn write_to_clone() {
    let tempfile = tempfile();

    let file = File::create(tempfile.path()).await.unwrap();
    let mut clone = file.try_clone().await.unwrap();

    clone.write_all(HELLO).await.unwrap();
    clone.flush().await.unwrap();

    let contents = std::fs::read(tempfile.path()).unwrap();
    assert_eq!(contents, HELLO);
}

#[tokio::test]
async fn write_into_std() {
    let tempfile = tempfile();

    let file = File::create(tempfile.path()).await.unwrap();
    let mut std_file = file.into_std().await;

    std_file.write_all(HELLO).unwrap();

    let contents = std::fs::read(tempfile.path()).unwrap();
    assert_eq!(contents, HELLO);
}

#[tokio::test]
async fn write_into_std_immediate() {
    let tempfile = tempfile();

    let file = File::create(tempfile.path()).await.unwrap();
    let mut std_file = file.try_into_std().unwrap();

    std_file.write_all(HELLO).unwrap();

    let contents = std::fs::read(tempfile.path()).unwrap();
    assert_eq!(contents, HELLO);
}

#[tokio::test]
async fn read_file_from_std() {
    let mut tempfile = tempfile();
    tempfile.write_all(HELLO).unwrap();

    let std_file = std::fs::File::open(tempfile.path()).unwrap();
    let mut file = File::from(std_file);

    let mut buf = [0; 1024];
    let n = file.read(&mut buf).await.unwrap();
    assert_eq!(n, HELLO.len());
    assert_eq!(&buf[..n], HELLO);
}

#[tokio::test]
async fn empty_read() {
    let mut tempfile = tempfile();
    tempfile.write_all(HELLO).unwrap();

    let mut file = File::open(tempfile.path()).await.unwrap();

    // Perform an empty read and get a length of zero.
    assert!(matches!(file.read(&mut []).now_or_never(), Some(Ok(0))));

    // Check that we don't get EOF on the next read.
    let mut buf = [0; 1024];
    let n = file.read(&mut buf).await.unwrap();

    assert_eq!(n, HELLO.len());
    assert_eq!(&buf[..n], HELLO);
}

fn tempfile() -> NamedTempFile {
    NamedTempFile::new().unwrap()
}

#[tokio::test]
async fn set_max_buf_size_read() {
    let mut tempfile = tempfile();
    tempfile.write_all(HELLO).unwrap();
    let mut file = File::open(tempfile.path()).await.unwrap();
    let mut buf = [0; 1024];
    file.set_max_buf_size(1);

    // A single read operation reads a maximum of 1 byte.
    assert_eq!(file.read(&mut buf).await.unwrap(), 1);
}

#[tokio::test]
async fn set_max_buf_size_write() {
    let tempfile = tempfile();
    let mut file = File::create(tempfile.path()).await.unwrap();
    file.set_max_buf_size(1);

    // A single write operation writes a maximum of 1 byte.
    assert_eq!(file.write(HELLO).await.unwrap(), 1);
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
#[cfg(unix)]
async fn file_debug_fmt() {
    let tempfile = tempfile();

    let file = File::open(tempfile.path()).await.unwrap();

    assert_eq!(
        &format!("{file:?}")[0..33],
        "tokio::fs::File { std: File { fd:"
    );
}

#[tokio::test]
#[cfg(windows)]
async fn file_debug_fmt() {
    let tempfile = tempfile();

    let file = File::open(tempfile.path()).await.unwrap();

    assert_eq!(
        &format!("{:?}", file)[0..37],
        "tokio::fs::File { std: File { handle:"
    );
}

#[tokio::test]
#[cfg(unix)]
async fn unix_fd_is_valid() {
    use std::os::unix::io::AsRawFd;
    let tempfile = tempfile();

    let file = File::create(tempfile.path()).await.unwrap();
    assert!(file.as_raw_fd() as u64 > 0);
}

#[tokio::test]
#[cfg(unix)]
async fn read_file_from_unix_fd() {
    use std::os::unix::io::{FromRawFd, IntoRawFd};

    let mut tempfile = tempfile();
    tempfile.write_all(HELLO).unwrap();

    let file1 = File::open(tempfile.path()).await.unwrap();
    let raw_fd = file1.into_std().await.into_raw_fd();
    assert!(raw_fd > 0);

    let mut file2 = unsafe { File::from_raw_fd(raw_fd) };

    let mut buf = [0; 1024];
    let n = file2.read(&mut buf).await.unwrap();
    assert_eq!(n, HELLO.len());
    assert_eq!(&buf[..n], HELLO);
}

#[tokio::test]
#[cfg(unix)]
async fn read_at_reads_correct_bytes_at_offset() {
    let mut temp_file = tempfile();
    temp_file.write_all(HELLO).unwrap();

    let file = File::open(temp_file.path()).await.unwrap();

    let offset = 1u64;
    let want = &HELLO[offset as usize..offset as usize + 4];
    let mut buf = vec![0u8; want.len()];
    let mut filled = 0;

    while filled < buf.len() {
        let n = file
            .read_at(&mut buf[filled..], offset + filled as u64)
            .await
            .unwrap();
        assert!(n > 0, "unexpected EOF before buffer was filled");
        filled += n;
    }

    assert_eq!(&buf[..], want);
}

#[tokio::test]
#[cfg(unix)]
async fn read_at_does_not_move_the_cursor() {
    let mut temp_file = tempfile();
    temp_file.write_all(HELLO).unwrap();

    let mut file = File::open(temp_file.path()).await.unwrap();
    file.seek(SeekFrom::Start(3)).await.unwrap();

    let mut buf = [0u8; 2];
    file.read_at(&mut buf, 0).await.unwrap();

    let pos = file.seek(SeekFrom::Current(0)).await.unwrap();
    assert_eq!(pos, 3, "read_at must not affect the file's cursor");
}

#[tokio::test]
#[cfg(unix)]
async fn read_at_short_read_at_eof() {
    let mut temp_file = tempfile();
    temp_file.write_all(HELLO).unwrap();

    let file = File::open(temp_file.path()).await.unwrap();
    let len = HELLO.len();

    let mut buf = vec![0u8; len + 10]; // buffer larger than remaining data
    let mut filled = 0;

    while filled < buf.len() {
        let n = file
            .read_at(&mut buf[filled..], filled as u64)
            .await
            .unwrap();

        // it's expected a short read here.
        if n == 0 {
            break;
        }

        filled += n;
    }

    assert_eq!(
        filled, len,
        "short read at EOF must return Ok(n) with n < buf.len(), not an error"
    );
    assert_eq!(&buf[..filled], HELLO);
}

#[tokio::test]
#[cfg(unix)]
async fn read_at_offset_at_eof_returns_zero() {
    let mut temp_file = tempfile();
    temp_file.write_all(HELLO).unwrap();

    let file = File::open(temp_file.path()).await.unwrap();
    let mut buf = [0u8; 4];
    let n = file.read_at(&mut buf, HELLO.len() as u64).await.unwrap();

    assert_eq!(n, 0);
}

#[tokio::test]
#[cfg(unix)]
async fn read_at_offset_past_eof_returns_zero() {
    let mut temp_file = tempfile();
    temp_file.write_all(HELLO).unwrap();

    let file = File::open(temp_file.path()).await.unwrap();
    let mut buf = [0u8; 4];
    let n = file
        .read_at(&mut buf, HELLO.len() as u64 + 100)
        .await
        .unwrap();

    assert_eq!(n, 0, "pread(2) past EOF returns 0, not an error");
}

#[tokio::test]
#[cfg(unix)]
async fn read_at_with_empty_buffer_returns_zero() {
    let mut temp_file = tempfile();
    temp_file.write_all(HELLO).unwrap();

    let file = File::open(temp_file.path()).await.unwrap();
    let mut buf: [u8; 0] = [];
    let n = file.read_at(&mut buf, 0).await.unwrap();

    assert_eq!(n, 0);
}

#[tokio::test]
#[cfg(unix)]
async fn read_at_calls_can_run_concurrently() {
    let mut temp_file = tempfile();
    temp_file.write_all(HELLO).unwrap();

    let file = File::open(temp_file.path()).await.unwrap();
    let mid = HELLO.len() / 2;

    let mut buf_a = vec![0u8; mid];
    let mut buf_b = vec![0u8; HELLO.len() - mid];
    let mut filled_a = 0;
    let mut filled_b = 0;

    while filled_a < buf_a.len() || filled_b < buf_b.len() {
        let fut_a = async {
            if filled_a < buf_a.len() {
                Some(file.read_at(&mut buf_a[filled_a..], filled_a as u64).await)
            } else {
                None
            }
        };

        let fut_b = async {
            if filled_b < buf_b.len() {
                Some(
                    file.read_at(&mut buf_b[filled_b..], mid as u64 + filled_b as u64)
                        .await,
                )
            } else {
                None
            }
        };

        let (ra, rb) = tokio::join!(fut_a, fut_b,);

        if let Some(a) = ra {
            let a = a.unwrap();
            assert!(a > 0, "unexpected EOF before buffer was filled");
            filled_a += a;
        }

        if let Some(b) = rb {
            let b = b.unwrap();
            assert!(b > 0, "unexpected EOF before buffer was filled");
            filled_b += b;
        }
    }

    assert_eq!(filled_a, mid);
    assert_eq!(filled_b, HELLO.len() - mid);
    assert_eq!(buf_a, &HELLO[..mid]);
    assert_eq!(buf_b, &HELLO[mid..]);
}

use std::io::Write;
#[cfg(unix)]
use tokio::fs::OpenOptions;

#[tokio::test]
#[cfg(unix)]
async fn write_at_overwrites_in_place_without_truncating() {
    let mut temp_file = tempfile();
    temp_file.write_all(HELLO).unwrap();

    let file = OpenOptions::new()
        .write(true)
        .open(temp_file.path())
        .await
        .unwrap();

    let patch: &[u8] = &[0xAA, 0xBB, 0xCC];
    let offset = 1usize;
    assert!(
        offset + patch.len() < HELLO.len(),
        "test setup: patch must leave trailing bytes untouched"
    );

    let mut written = 0;

    while written < patch.len() {
        let n = file
            .write_at(&patch[written..], offset as u64 + written as u64)
            .await
            .unwrap();
        assert!(n > 0, "unexpected zero write before buffer was flushed");
        written += n;
    }

    assert_eq!(written, patch.len());
    let on_disk = std::fs::read(temp_file.path()).unwrap();
    assert_eq!(
        on_disk.len(),
        HELLO.len(),
        "write_at must not change file length when writing inside existing bounds"
    );

    assert_eq!(&on_disk[..offset], &HELLO[..offset]);
    assert_eq!(&on_disk[offset..offset + patch.len()], patch);
    assert_eq!(
        &on_disk[offset + patch.len()..],
        &HELLO[offset + patch.len()..]
    );

    let n = file.write_at(patch, offset as u64).await.unwrap();
    assert_eq!(n, patch.len());

    let on_disk = std::fs::read(temp_file.path()).unwrap();
    assert_eq!(
        on_disk.len(),
        HELLO.len(),
        "write_at must not change file length when writing inside existing bounds"
    );
    assert_eq!(&on_disk[..offset], &HELLO[..offset]);
    assert_eq!(&on_disk[offset..offset + patch.len()], patch);
    assert_eq!(
        &on_disk[offset + patch.len()..],
        &HELLO[offset + patch.len()..]
    );
}

#[tokio::test]
#[cfg(unix)]
async fn write_at_does_not_move_cursor() {
    let mut temp_file = tempfile();
    temp_file.write_all(HELLO).unwrap();

    let mut file = OpenOptions::new()
        .write(true)
        .open(temp_file.path())
        .await
        .unwrap();

    file.seek(SeekFrom::Start(4)).await.unwrap();

    file.write_at(&[9, 9, 9], 0).await.unwrap();

    let pos = file.seek(SeekFrom::Current(0)).await.unwrap();
    assert_eq!(pos, 4, "write_at must not affect the file's cursor");
}

#[tokio::test]
#[cfg(unix)]
async fn write_at_extends_file_and_zero_fills_gap() {
    let temp_file = tempfile();

    let file = OpenOptions::new()
        .write(true)
        .open(temp_file.path())
        .await
        .unwrap();

    let data = b"end";
    let offset = 5u64;
    let mut written = 0;

    while written < data.len() {
        let n = file
            .write_at(&data[written..], offset + written as u64)
            .await
            .unwrap();
        assert!(n > 0, "unexpected zero write before buffer was flushed");
        written += n;
    }

    assert_eq!(written, data.len());
    let on_disk = std::fs::read(temp_file.path()).unwrap();
    assert_eq!(on_disk.len(), offset as usize + data.len());
    assert_eq!(&on_disk[..offset as usize], &[0u8; 5]);
    assert_eq!(&on_disk[offset as usize..], data);
}

#[tokio::test]
#[cfg(unix)]
async fn write_at_with_empty_buffer_returns_zero() {
    let temp_file = tempfile();

    let file = OpenOptions::new()
        .write(true)
        .open(temp_file.path())
        .await
        .unwrap();

    let n = file.write_at(&[], 0).await.unwrap();
    assert_eq!(n, 0);

    let on_disk = std::fs::read(temp_file.path()).unwrap();
    assert!(
        on_disk.is_empty(),
        "writing an empty buffer must not extend the file"
    );
}

#[tokio::test]
#[cfg(unix)]
async fn write_at_calls_can_run_concurrently() {
    let temp_file = tempfile();

    let file = OpenOptions::new()
        .write(true)
        .open(temp_file.path())
        .await
        .unwrap();

    let expect_bytes = 4;
    let mut filled_a = 0;
    let mut filled_b = 0;

    while filled_a < expect_bytes || filled_b < expect_bytes {
        let buffer_a = b"AAAA";
        let buffer_b = b"BBBB";

        let (ra, rb) = tokio::join!(
            file.write_at(&buffer_a[filled_a..], filled_a as u64),
            file.write_at(&buffer_b[filled_b..], 4 + filled_b as u64)
        );
        let ra = ra.unwrap();
        let rb = rb.unwrap();

        filled_a += ra;
        filled_b += rb;
    }

    assert_eq!(filled_a, expect_bytes);
    assert_eq!(filled_b, expect_bytes);

    let on_disk = std::fs::read(temp_file.path()).unwrap();
    assert_eq!(&on_disk[..4], b"AAAA");
    assert_eq!(&on_disk[4..8], b"BBBB");
}

#[tokio::test]
#[cfg(windows)]
async fn windows_handle() {
    use std::os::windows::io::AsRawHandle;
    let tempfile = tempfile();

    let file = File::create(tempfile.path()).await.unwrap();
    assert!(file.as_raw_handle() as u64 > 0);
}
