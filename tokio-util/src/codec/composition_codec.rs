use bytes::BytesMut;

use crate::codec::{Decoder, Encoder};

/// A [`Decoder`] that composes two decoders into a single decoder.
///
/// The first decoder must output [`BytesMut`], to be connected to the second one.
///
/// This decoder will hold the intermediate result of the first decoder in a buffer.
///
/// [`Decoder`]: crate::codec::Decoder
#[derive(Debug)]
pub struct CompositionDecoder<D1, D2>
where
    D1: Decoder<Item = BytesMut>,
    D2: Decoder,
{
    first: D1,
    second: D2,
    intermediate_buffer: BytesMut,
}

impl<D1, D2> CompositionDecoder<D1, D2>
where
    D1: Decoder<Item = BytesMut>,
    D2: Decoder,
{
    /// Creates a new [`CompositionDecoder`] that will use `first` to decode data
    /// and then pass the result to `second`.
    pub fn new(first: D1, second: D2) -> Self {
        Self {
            first,
            second,
            intermediate_buffer: BytesMut::new(),
        }
    }
}

impl<D1, D2> Decoder for CompositionDecoder<D1, D2>
where
    D1: Decoder<Item = BytesMut>,
    D2: Decoder,
{
    type Item = D2::Item;

    type Error = CompositionDecoderError<D1, D2>;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // First, check if we can decode using our buffer
        if !self.intermediate_buffer.is_empty() {
            if let Some(result) = self
                .second
                .decode(&mut self.intermediate_buffer)
                .map_err(CompositionDecoderError::D2)?
            {
                return Ok(Some(result));
            }
        }

        // Then, try to load more data into the second decoder's buffer using the first decoder
        if let Some(intermediate) = self
            .first
            .decode(src)
            .map_err(CompositionDecoderError::D1)?
        {
            // Add the intermediate result to our buffer
            self.intermediate_buffer.extend_from_slice(&intermediate);

            // Now retry to decode using the second decoder
            return self
                .second
                .decode(&mut self.intermediate_buffer)
                .map_err(CompositionDecoderError::D2);
        }

        Ok(None)
    }
}

#[derive(Debug)]
pub enum CompositionDecoderError<D1, D2>
where
    D1: Decoder,
    D2: Decoder,
{
    Io(std::io::Error),
    D1(D1::Error),
    D2(D2::Error),
}

impl<D1, D2> From<std::io::Error> for CompositionDecoderError<D1, D2>
where
    D1: Decoder,
    D2: Decoder,
{
    fn from(err: std::io::Error) -> Self {
        CompositionDecoderError::Io(err)
    }
}

/// Extension trait for [`Decoder`] that allows composing a decoder after `self`.
pub trait DecoderCompositionExt: Decoder<Item = BytesMut> {
    /// Compose a decoder after `self`.
    fn compose_decoder<D>(self, other: D) -> CompositionDecoder<Self, D>
    where
        Self: Sized,
        D: Decoder,
    {
        CompositionDecoder::new(self, other)
    }
}

impl<D> DecoderCompositionExt for D
where
    D: Decoder<Item = BytesMut>,
{
    fn compose_decoder<D2>(self, other: D2) -> CompositionDecoder<Self, D2>
    where
        Self: Sized,
        D2: Decoder,
    {
        CompositionDecoder::new(self, other)
    }
}

/// An [`Encoder`] that composes two encoders into a single encoder.
///
/// The second encoder must take [`BytesMut`], to be connected to the first one.
///
/// [`Encoder`]: crate::codec::Encoder
#[derive(Debug)]
pub struct CompositionEncoder<E1, E2> {
    first: E1,
    second: E2,
}

impl<E1, E2> CompositionEncoder<E1, E2> {
    /// Creates a new [`CompositionEncoder`] that will use `first` to encode data
    /// and then pass the result to `second`.
    pub fn new(first: E1, second: E2) -> Self {
        Self { first, second }
    }
}

impl<E1, E2, Item> Encoder<Item> for CompositionEncoder<E1, E2>
where
    E1: Encoder<Item>,
    E2: Encoder<BytesMut>,
{
    type Error = CompositionEncoderError<E1, E2, Item>;

    fn encode(&mut self, item: Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let mut intermediate = BytesMut::new();

        // First, try to encode using our first encoder
        self.first
            .encode(item, &mut intermediate)
            .map_err(CompositionEncoderError::E1)?;

        // Then, try to encode using our second encoder
        self.second
            .encode(intermediate, dst)
            .map_err(CompositionEncoderError::E2)?;

        Ok(())
    }
}

#[derive(Debug)]
pub enum CompositionEncoderError<E1, E2, Item>
where
    E1: Encoder<Item>,
    E2: Encoder<BytesMut>,
{
    Io(std::io::Error),
    E1(E1::Error),
    E2(E2::Error),
}

impl<E1, E2, Item> From<std::io::Error> for CompositionEncoderError<E1, E2, Item>
where
    E1: Encoder<Item>,
    E2: Encoder<BytesMut>,
{
    fn from(err: std::io::Error) -> Self {
        CompositionEncoderError::Io(err)
    }
}

/// Extension trait for [`Encoder`] that allows composing a encoder after `self`.
pub trait EncoderCompositionExt<E>: Encoder<E> {
    /// Compose a encoder after `self`.
    fn compose_encoder<D>(self, other: D) -> CompositionEncoder<Self, D>
    where
        Self: Sized,
        D: Encoder<BytesMut>,
    {
        CompositionEncoder::new(self, other)
    }
}

impl<D, E> EncoderCompositionExt<E> for D
where
    D: Encoder<E>,
{
    fn compose_encoder<E2>(self, other: E2) -> CompositionEncoder<Self, E2>
    where
        Self: Sized,
        E2: Encoder<BytesMut>,
    {
        CompositionEncoder::new(self, other)
    }
}
