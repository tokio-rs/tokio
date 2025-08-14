use bytes::{BufMut, BytesMut};
use tokio_util::codec::{Encoder, EncoderCompositionExt};

#[derive(Debug)]
struct StringEncoder {}

impl Encoder<String> for StringEncoder {
    type Error = std::io::Error;

    fn encode(&mut self, item: String, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let length: u32 = item.len().try_into().expect("length is too large");
        dst.put_u32(length);
        dst.put(item.as_bytes());
        Ok(())
    }
}

#[derive(Debug)]
struct BitwiseNotEncoder {}

impl Encoder<BytesMut> for BitwiseNotEncoder {
    type Error = std::io::Error;

    fn encode(&mut self, mut item: BytesMut, dst: &mut BytesMut) -> Result<(), Self::Error> {
        bitwise_not_in_place(&mut item);
        dst.extend_from_slice(&item);
        Ok(())
    }
}

fn bitwise_not_in_place(buffer: &mut BytesMut) {
    for byte in buffer.iter_mut() {
        *byte = !*byte;
    }
}

#[test]
fn test_simple_chain() {
    let mut encoder = StringEncoder {}.compose_encoder(BitwiseNotEncoder {});

    let string = String::from("Hello, world!");
    let mut buffer = BytesMut::new();
    buffer.put_u32(string.len() as u32);
    buffer.put(string.as_bytes());

    bitwise_not_in_place(&mut buffer);
    let mut dst = BytesMut::new();

    encoder.encode(string, &mut dst).unwrap();
    assert_eq!(dst, buffer);
}

#[test]
fn test_not_not() {
    let mut encoder = StringEncoder {}
        .compose_encoder(BitwiseNotEncoder {})
        .compose_encoder(BitwiseNotEncoder {})
        .compose_encoder(BitwiseNotEncoder {});

    let string = String::from("Hello, world!");
    let mut buffer = BytesMut::new();
    buffer.put_u32(string.len() as u32);
    buffer.put(string.as_bytes());

    bitwise_not_in_place(&mut buffer);
    let mut dst = BytesMut::new();

    encoder.encode(string, &mut dst).unwrap();
    assert_eq!(dst, buffer);
}
