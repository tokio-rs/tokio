/// Can create buffers of arbitrary lifetime.
/// Frees created buffers when dropped.
///
/// This struct is of course unsafe and the fact that
/// it outlives the created buffers has to be managed by
/// the programmer.
///
/// Used at certain test scenarios as a safer version of
/// Vec::leak, to satisfy the address sanitizer.
pub struct LeakedBuffers {
    leaked_vecs: Vec<(*const u8, usize)>,
}

impl LeakedBuffers {
    pub fn new() -> Self {
        Self {
            leaked_vecs: vec![],
        }
    }
    pub fn create<'a>(&mut self, size: usize) -> &'a mut [u8] {
        let slice = Box::leak(vec![0; size].into_boxed_slice());
        self.leaked_vecs.push((slice.as_ptr(), slice.len()));
        slice
    }
}

impl Drop for LeakedBuffers {
    fn drop(&mut self) {
        for (ptr, len) in self.leaked_vecs.drain(..) {
            unsafe {
                Box::from_raw(std::slice::from_raw_parts_mut(ptr as *mut u8, len));
            }
        }
    }
}
