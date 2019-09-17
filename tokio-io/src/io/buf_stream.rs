use crate::io::{BufReader, BufWriter};
use crate::{AsyncRead, AsyncWrite};

/// Wraps a type that is [`AsyncWrite`] and [`AsyncRead`], and buffers its input and output.
///
/// It can be excessively inefficient to work directly with something that implements [`AsyncWrite`]
/// and [`AsyncRead`]. For example, every `write`, however small, has to traverse the syscall
/// interface, and similarly, every read has to do the same. The [`BufWriter`] and [`BufReader`]
/// types aid with these problems respectively, but do so in only one direction. `BufStream` wraps
/// one in the other so that both directions are buffered. See their documentation for details.
pub type BufStream<RW> = BufReader<BufWriter<RW>>;

/// Wrap a type in both [`BufWriter`] and [`BufReader`].
///
/// See the documentation for those types and [`BufStream`] for details.
pub fn buffer<RW: AsyncRead + AsyncWrite>(stream: RW) -> BufStream<RW> {
    BufReader::new(BufWriter::new(stream))
}
