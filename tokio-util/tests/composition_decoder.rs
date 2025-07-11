use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, DecoderCompositionExt};

#[derive(Debug)]
struct StringDecoder {}

impl Decoder for StringDecoder {
    type Item = String;

    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < size_of::<u32>() {
            return Ok(None);
        }

        let length: usize = src.get_u32().try_into().expect("length is too large");
        if src.len() < length {
            return Ok(None);
        }

        let string = src.split_to(length);
        Ok(Some(
            String::from_utf8(string.to_vec()).expect("string is not valid utf8"),
        ))
    }
}

#[derive(Debug)]
struct BitwiseNotDecoder {}

impl Decoder for BitwiseNotDecoder {
    type Item = BytesMut;

    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let mut buffer = src.split();

        bitwise_not_in_place(&mut buffer);

        Ok(Some(buffer))
    }
}

fn bitwise_not_in_place(buffer: &mut BytesMut) {
    for byte in buffer.iter_mut() {
        *byte = !*byte;
    }
}

#[test]
fn test_simple_chain() {
    let mut decoder = BitwiseNotDecoder {}.compose_decoder(StringDecoder {});

    let string = String::from("Hello, world!");
    let mut buffer = BytesMut::new();
    buffer.put_u32(string.len() as u32);
    buffer.put(string.as_bytes());

    bitwise_not_in_place(&mut buffer);

    let result = decoder.decode(&mut buffer).unwrap().unwrap();
    assert_eq!(result, string);
}

#[test]
fn test_not_not_not() {
    let mut decoder = BitwiseNotDecoder {}
        .compose_decoder(BitwiseNotDecoder {})
        .compose_decoder(BitwiseNotDecoder {})
        .compose_decoder(StringDecoder {});

    let string = String::from("Hello, world!");
    let mut buffer = BytesMut::new();
    buffer.put_u32(string.len() as u32);
    buffer.put(string.as_bytes());

    bitwise_not_in_place(&mut buffer);

    let result = decoder.decode(&mut buffer).unwrap().unwrap();
    assert_eq!(result, string);
}
