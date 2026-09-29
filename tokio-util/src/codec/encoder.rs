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
    /// `dst` is reused across calls, so an encoder that handles items of highly
    /// varying size can grow it to the size of the largest item encoded so far.
    /// `BytesMut` cannot release spare capacity in place, so that allocation is
    /// retained for the lifetime of the [`FramedWrite`].
    ///
    /// An encoder that can produce items much larger than usual can drop that
    /// capacity by replacing the buffer, but it must preserve any bytes that
    /// [`FramedWrite`] has not yet written out:
    ///
    /// ```
    /// use bytes::BytesMut;
    ///
    /// fn reclaim(dst: &mut BytesMut) {
    ///     *dst = BytesMut::from(&dst[..]);
    /// }
    ///
    /// let mut dst = BytesMut::new();
    /// dst.resize(1024 * 1024, 0u8);
    /// dst.truncate(3);
    /// dst[0..3].copy_from_slice(b"abc");
    ///
    /// let before = dst.capacity();
    /// reclaim(&mut dst);
    ///
    /// assert_eq!(&dst[..], b"abc");
    /// assert!(dst.capacity() < before);
    /// ```
    ///
    /// This copies the unconsumed bytes, so it should only be done when the
    /// retained capacity is actually a problem, and not on every call. In most
    /// cases, letting the buffer keep its capacity is the cheaper choice.
    ///
    /// [`FramedWrite`]: crate::codec::FramedWrite
    fn encode(&mut self, item: Item, dst: &mut BytesMut) -> Result<(), Self::Error>;
}
