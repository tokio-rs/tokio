use crate::fs::OpenOptions;
use crate::runtime::driver::op::Op;

use std::io;
use std::io::ErrorKind;
use std::os::fd::OwnedFd;
use std::path::Path;

// this algorithm is inspired from rust std lib version 1.90.0
// https://doc.rust-lang.org/1.90.0/src/std/io/mod.rs.html#409
const PROBE_SIZE: usize = 32;
const PROBE_SIZE_U32: u32 = PROBE_SIZE as u32;

// Max bytes we can read using io uring submission at a time
// SAFETY: cannot be higher than u32::MAX for safe cast
// Set to read max 64 blocks at time
const MAX_READ_SIZE: usize = 64 * 1024 * 1024;

pub(crate) async fn read_uring(path: &Path) -> io::Result<Vec<u8>> {
    let file = OpenOptions::new().read(true).open(path).await?;

    // TODO: use io uring in the future to obtain metadata
    let size_hint: Option<usize> = file.metadata().await.map(|m| m.len() as usize).ok();

    let fd: OwnedFd = file
        .try_into_std()
        .expect("unexpected in-flight operation detected")
        .into();

    // extra single capacity for the whole size to fit without any reallocation
    let buf = Vec::with_capacity(size_hint.unwrap_or(0));

    read_to_end_uring(size_hint, fd, buf).await
}

async fn read_to_end_uring(
    size_hint: Option<usize>,
    mut fd: OwnedFd,
    mut buf: Vec<u8>,
) -> io::Result<Vec<u8>> {
    let mut offset = 0;
    let start_cap = buf.capacity();

    // if buffer has no room and no size_hint, start with a small probe_read from 0 offset
    if (size_hint.is_none() || size_hint == Some(0)) && buf.capacity() - buf.len() < PROBE_SIZE {
        let (size_read, r_fd, r_buf) = small_probe_read(fd, buf, &mut offset).await?;

        if size_read == 0 {
            return Ok(r_buf);
        }

        fd = r_fd;
        buf = r_buf;
    }

    loop {
        if buf.len() == buf.capacity() && buf.capacity() == start_cap {
            // The buffer might be an exact fit. Let's read into a probe buffer
            // and see if it returns `Ok(0)`. If so, we've avoided an
            // unnecessary increasing of the capacity. But if not, append the
            // probe buffer to the primary buffer and let its capacity grow.
            let (size_read, r_fd, r_buf) = small_probe_read(fd, buf, &mut offset).await?;

            if size_read == 0 {
                return Ok(r_buf);
            }

            buf = r_buf;
            fd = r_fd;
            offset += size_read as u64;
        }

        // buf is full, need more capacity
        if buf.len() == buf.capacity() {
            buf.try_reserve(PROBE_SIZE)?;
        }

        // prepare the spare capacity to be read into
        let buf_len = usize::min(buf.spare_capacity_mut().len(), MAX_READ_SIZE);

        // buf_len cannot be greater than u32::MAX because MAX_READ_SIZE
        // is u32::MAX
        let mut read_len = buf_len as u32;

        // read into spare capacity
        let (size_read, r_fd, r_buf) = op_read(fd, buf, &mut offset, &mut read_len).await?;

        if size_read == 0 {
            return Ok(r_buf);
        }

        fd = r_fd;
        buf = r_buf;
    }
}

async fn small_probe_read(
    fd: OwnedFd,
    mut buf: Vec<u8>,
    offset: &mut u64,
) -> io::Result<(u32, OwnedFd, Vec<u8>)> {
    let mut read_len = PROBE_SIZE_U32;

    if buf.len() < PROBE_SIZE {
        buf.try_reserve(PROBE_SIZE)?;

        return op_read(fd, buf, offset, &mut read_len).await;
    }

    let mut temp_arr = [0; PROBE_SIZE];
    let back_bytes_len = buf.len() - PROBE_SIZE;

    temp_arr.copy_from_slice(&buf[back_bytes_len..]);

    // We're decreasing the length of the buffer and len is greater
    // than PROBE_SIZE. So we can read into the discarded length
    buf.truncate(back_bytes_len);

    let (size_read, r_fd, mut r_buf) = op_read(fd, buf, offset, &mut read_len).await?;

    r_buf.splice(back_bytes_len..back_bytes_len, temp_arr);

    Ok((size_read, r_fd, r_buf))
}

async fn op_read(
    mut fd: OwnedFd,
    mut buf: Vec<u8>,
    offset: &mut u64,
    read_len: &mut u32,
) -> io::Result<(u32, OwnedFd, Vec<u8>)> {
    loop {
        let (res, r_fd, r_buf) = Op::read(fd, buf, *read_len, *offset).await;

        match res {
            Err(e) if e.kind() == ErrorKind::Interrupted => {
                buf = r_buf;
                fd = r_fd;
            }
            Err(e) => return Err(e),
            Ok(size_read) => {
                *offset += size_read as u64;
                *read_len -= size_read;

                if *read_len == 0 || size_read == 0 {
                    return Ok((size_read, r_fd, r_buf));
                }

                buf = r_buf;
                fd = r_fd;
            }
        }
    }
}
