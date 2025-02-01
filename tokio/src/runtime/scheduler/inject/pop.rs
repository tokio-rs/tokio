use super::Synced;

use crate::runtime::task;

use std::marker::PhantomData;

pub(crate) struct Pop<'a, T: 'static> {
    len: usize,
    synced: &'a mut Synced,
    _p: PhantomData<T>,
}

impl<'a, T: 'static> Pop<'a, T> {
    pub(super) fn new(len: usize, synced: &'a mut Synced) -> Pop<'a, T> {
        Pop {
            len,
            synced,
            _p: PhantomData,
        }
    }
}

impl<T: 'static> Iterator for Pop<'_, T> {
    type Item = task::Notified<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }

        let ret = self.synced.pop();

        // Should be `Some` when `len > 0`
        debug_assert!(ret.is_some());

        self.len -= 1;
        ret
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<T: 'static> ExactSizeIterator for Pop<'_, T> {
    fn len(&self) -> usize {
        self.len
    }
}

impl<T: 'static> Drop for Pop<'_, T> {
    fn drop(&mut self) {
        for _ in self.by_ref() {}
    }
}
