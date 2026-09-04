use bytes::BytesMut;
use std::io;

/// Trait of helper objects to write out messages as bytes, for use with
/// [`FramedWrite`].
///
/// [`FramedWrite`]: crate::codec::FramedWrite
pub trait Encoder<Item> {
    /// The type of encoding errors.
    ///
    /// [`FramedWrite`] requires `Encoder`s errors to implement `From<io::Error>`
    /// in the interest of letting it return `Error`s directly.
    ///
    /// [`FramedWrite`]: crate::codec::FramedWrite
    type Error: From<io::Error>;

    /// Encodes a frame into the buffer provided.
    ///
    /// This method will encode `item` into the byte buffer provided by `dst`.
    /// The `dst` provided is an internal buffer of the [`FramedWrite`] instance and
    /// will be written out when possible.
    ///
    /// # Buffer management
    ///
    /// The buffer is reused across calls and may retain its allocation after a
    /// large frame is written. If the encoder replaces `dst` to reclaim that
    /// allocation, it must preserve any queued data. The [`Decoder::decode`]
    /// buffer management guidance shows how to replace a buffer while preserving
    /// its contents.
    ///
    /// [`FramedWrite`]: crate::codec::FramedWrite
    /// [`Decoder::decode`]: crate::codec::Decoder::decode
    fn encode(&mut self, item: Item, dst: &mut BytesMut) -> Result<(), Self::Error>;
}
