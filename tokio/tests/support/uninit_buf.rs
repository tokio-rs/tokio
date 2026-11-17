use bytes::buf::UninitSlice;
use bytes::BufMut;
use std::mem::MaybeUninit;

pub struct UninitBuf<const N: usize> {
    buf: [MaybeUninit<u8>; N],
    len: usize,
    max_chunk: usize,
    pub advance_calls: usize,
}

impl<const N: usize> UninitBuf<N> {
    pub fn new(max_chunk: usize) -> Self {
        Self {
            buf: [MaybeUninit::uninit(); N],
            len: 0,
            max_chunk,
            advance_calls: 0,
        }
    }

    pub fn initialized(&self) -> &[u8] {
        // Safety: `buf[..len]` has been initialized by callers via `advance_mut`.
        unsafe { std::slice::from_raw_parts(self.buf.as_ptr().cast(), self.len) }
    }
}

unsafe impl<const N: usize> BufMut for UninitBuf<N> {
    fn remaining_mut(&self) -> usize {
        N - self.len
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        assert!(cnt <= N - self.len);
        self.len += cnt;
        self.advance_calls += 1;
    }

    fn chunk_mut(&mut self) -> &mut UninitSlice {
        let end = (self.len + self.max_chunk).min(N);
        UninitSlice::uninit(&mut self.buf[self.len..end])
    }
}
