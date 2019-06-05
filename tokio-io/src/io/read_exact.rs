use crate::AsyncRead;
use std::{io, mem};

/// Creates a future which will read exactly enough bytes to fill `buf`,
/// returning an error if EOF is hit sooner.
///
/// In the case of an error the buffer and the object will be discarded, with
/// the error yielded. In the case of success the object will be destroyed and
/// the buffer will be returned, with all data read from the stream appended to
/// the buffer.
pub async fn read_exact<'a, R>(reader: &'a mut R, mut buf: &'a mut [u8]) -> io::Result<()>
where
    R: AsyncRead + Unpin + ?Sized,
{
    while !buf.is_empty() {
        let n = super::read(reader, buf).await?;
        {
            let (_, rest) = mem::replace(&mut buf, &mut []).split_at_mut(n);
            buf = rest;
        }

        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
    }

    Ok(())
}
