use std::sync::Arc;

#[cfg(feature = "io-util")]
use bytes::Buf;

use super::typeid;

#[derive(Debug, Clone)]
pub(crate) enum OwnedBuf {
    Vec(VecBuf),
    #[cfg(feature = "io-util")]
    Bytes(bytes::Bytes),
}

#[allow(unused)]
impl OwnedBuf {
    pub(crate) fn len(&self) -> usize {
        match self {
            OwnedBuf::Vec(vec) => vec.len(),
            #[cfg(feature = "io-util")]
            OwnedBuf::Bytes(b) => b.len(),
        }
    }

    pub(crate) fn advance(&mut self, n: usize) {
        match self {
            OwnedBuf::Vec(vec) => vec.advance(n),
            #[cfg(feature = "io-util")]
            OwnedBuf::Bytes(b) => b.advance(n),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        match self {
            Self::Vec(vec) => vec.is_empty(),
            #[cfg(feature = "io-util")]
            Self::Bytes(bytes) => bytes.is_empty(),
        }
    }
}

/// A Vec-like, reference-counted buffer.
#[derive(Debug, Clone)]
pub(crate) struct VecBuf {
    data: Arc<[u8]>,
    pos: usize,
}

impl VecBuf {
    fn len(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn advance(&mut self, n: usize) {
        self.pos = self.pos.saturating_add(n);
    }

    fn is_empty(&self) -> bool {
        self.data.len().saturating_sub(self.pos) == 0
    }
}

impl AsRef<[u8]> for OwnedBuf {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Vec(VecBuf { data, pos }) => &data[*pos..],
            #[cfg(feature = "io-util")]
            Self::Bytes(bytes) => bytes,
        }
    }
}

pub(crate) fn upgrade<B: AsRef<[u8]>>(buf: B) -> OwnedBuf {
    let buf = match unsafe { typeid::try_transmute::<B, Vec<u8>>(buf) } {
        Ok(vec) => {
            return OwnedBuf::Vec(VecBuf {
                data: vec.into(),
                pos: 0,
            });
        }
        Err(original_buf) => original_buf,
    };

    let buf = match unsafe { typeid::try_transmute::<B, String>(buf) } {
        Ok(string) => {
            return OwnedBuf::Vec(VecBuf {
                data: string.into_bytes().into(),
                pos: 0,
            });
        }
        Err(original_buf) => original_buf,
    };

    #[cfg(feature = "io-util")]
    let buf = match unsafe { typeid::try_transmute::<B, bytes::Bytes>(buf) } {
        Ok(bytes) => return OwnedBuf::Bytes(bytes),
        Err(original_buf) => original_buf,
    };

    OwnedBuf::Vec(VecBuf {
        data: buf.as_ref().to_owned().into(),
        pos: 0,
    })
}
