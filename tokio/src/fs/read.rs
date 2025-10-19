use crate::fs::asyncify;

use std::{io, path::Path};

/// Reads the entire contents of a file into a bytes vector.
///
/// This is an async version of [`std::fs::read`].
///
/// This is a convenience function for using [`File::open`] and [`read_to_end`]
/// with fewer imports and without an intermediate variable. It pre-allocates a
/// buffer based on the file size when available, so it is generally faster than
/// reading into a vector created with `Vec::new()`.
///
/// This operation is implemented by running the equivalent blocking operation
/// on a separate thread pool using [`spawn_blocking`].
///
/// [`File::open`]: super::File::open
/// [`read_to_end`]: crate::io::AsyncReadExt::read_to_end
/// [`spawn_blocking`]: crate::task::spawn_blocking
///
/// # Errors
///
/// This function will return an error if `path` does not already exist.
/// Other errors may also be returned according to [`OpenOptions::open`].
///
/// [`OpenOptions::open`]: super::OpenOptions::open
///
/// It will also return an error if it encounters while reading an error
/// of a kind other than [`ErrorKind::Interrupted`].
///
/// [`ErrorKind::Interrupted`]: std::io::ErrorKind::Interrupted
///
/// # Examples
///
/// ```no_run
/// use tokio::fs;
/// use std::net::SocketAddr;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
///     let contents = fs::read("address.txt").await?;
///     let foo: SocketAddr = String::from_utf8_lossy(&contents).parse()?;
///     Ok(())
/// }
/// ```
pub async fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let path = path.as_ref().to_owned();

    #[cfg(all(
        tokio_unstable,
        feature = "io-uring",
        feature = "rt",
        feature = "fs",
        target_os = "linux"
    ))]
    {
        let handle = crate::runtime::Handle::current();
        let driver_handle = handle.inner.driver().io();
        if driver_handle.check_and_init()? {
            return read_uring(&path).await;
        }
    }

    asyncify(move || std::fs::read(path)).await
}

#[cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]
async fn read_uring(path: &Path) -> io::Result<Vec<u8>> {
    use crate::{fs::OpenOptions, runtime::driver::op::Op};

    use std::cmp;
    use std::io::ErrorKind;
    use std::os::fd::OwnedFd;

    // this algorithm is inspired from rust std lib version 1.90.0
    // https://doc.rust-lang.org/1.90.0/src/std/io/mod.rs.html#409
    const PROBE_SIZE: usize = 32;

    let file = OpenOptions::new().read(true).open(path).await?;

    let size_hint: Option<usize> = file
        .metadata()
        .await
        .map(|m| usize::try_from(m.len()).unwrap_or(usize::MAX))
        .ok();

    // Max bytes we can read using io uring submission at a time
    // SAFETY: cannot be higher than u32::MAX
    let max_read_size = u32::MAX as usize;

    let mut buf = Vec::with_capacity(size_hint.unwrap_or(0));

    let mut fd: OwnedFd = file
        .try_into_std()
        .expect("unexpected in-flight operation detected")
        .into();

    let mut offset = 0;
    let mut consecutive_short_reads = 0;
    let start_cap = buf.capacity();

    async fn small_probe_read(
        mut fd: OwnedFd,
        mut buf: Vec<u8>,
        offset: u64,
    ) -> io::Result<(u32, OwnedFd, Vec<u8>)> {
        let probe = [0u8; PROBE_SIZE];

        loop {
            let (res, _fd, _buf) = Op::read_exact_once(fd, probe, offset).await;

            fd = _fd;

            match res {
                Ok(_size_read) => {
                    buf.extend_from_slice(&probe[..(_size_read as usize)]);

                    return Ok((_size_read, fd, buf));
                }
                Err(e) => {
                    if e.kind() == ErrorKind::Interrupted {
                        continue;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
    }

    // if buffer has no room and no size_hint, start with a small probe_read from 0 offset
    if (size_hint.is_none() || size_hint == Some(0)) && buf.capacity() - buf.len() < PROBE_SIZE {
        let (_size_read, _fd, _buf) = small_probe_read(fd, buf, offset).await?;

        buf = _buf;

        if _size_read == 0 {
            return Ok(buf);
        }

        fd = _fd;
        offset += _size_read as u64;
    }

    loop {
        if buf.len() == buf.capacity() && buf.capacity() == start_cap {
            // The buffer might be an exact fit. Let's read into a probe buffer
            // and see if it returns `Ok(0)`. If so, we've avoided an
            // unnecessary increasing of the capacity. But if not, append the
            // probe buffer to the primary buffer and let its capacity grow.
            let (_size_read, _fd, _buf) = small_probe_read(fd, buf, offset).await?;

            buf = _buf;

            if _size_read == 0 {
                return Ok(buf);
            }

            fd = _fd;
            offset += _size_read as u64;
        }

        // buf is full, need more capacity
        if buf.len() == buf.capacity() {
            buf.try_reserve(PROBE_SIZE)?
        }

        // doesn't matter if we have a valid size_hint or not, if we do more
        // than 2 consecutive_short_reads, gradually increase the buffer
        // capacity to read more data at a time
        if consecutive_short_reads > 1 {
            buf.try_reserve(PROBE_SIZE.saturating_mul(consecutive_short_reads))?
        }

        // prepare the spare capacity to be read into
        // this is equal to PROBE_SIZE unless consecutive_short_reads > 1
        let spare = buf.capacity() - buf.len();
        let buf_len = cmp::min(spare, max_read_size);

        let mut read_len = {
            // SAFETY:
            // 1. buf_len cannot be greater than u32::MAX because max_read_size
            // is u32::MAX
            // 2. This is faster than a cast
            unsafe { TryInto::<u32>::try_into(buf_len).unwrap_unchecked() }
        };

        loop {
            // read into spare capacity
            let (res, _fd, _buf) = Op::read(fd, buf, read_len, offset).await;

            match res {
                Ok(_size_read) => {
                    let size_read_usize = _size_read as usize;
                    buf = _buf;

                    let new_len = size_read_usize + buf.len();
                    // SAFETY: We didn't read more than what as reserved
                    // as capacity, the _size_read was initialized by the kernel
                    // via a mutable pointer
                    unsafe { buf.set_len(new_len) }

                    if _size_read == 0 {
                        return Ok(buf);
                    }

                    fd = _fd;
                    offset += _size_read as u64;
                    read_len -= _size_read;

                    // 1. In case of no size_hint and a large file, if we keep reading
                    // PROBE_SIZE, we want to increment number of short reads in order to gradually
                    // increase read size per Op submission
                    // 2. In case of small reads by the kernel, also gradually increase
                    // read size per Op submission to read files in lesser cycles
                    if size_read_usize <= buf.len() {
                        consecutive_short_reads += 1;
                    } else {
                        consecutive_short_reads = 0;
                    }

                    // keep reading if there's something left to be read
                    if read_len > 0 {
                        continue;
                    } else {
                        break;
                    }
                }
                Err(e) => {
                    if e.kind() == ErrorKind::Interrupted {
                        buf = _buf;
                        fd = _fd;

                        continue;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
    }
}
