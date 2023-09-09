use super::Synced;

use crate::runtime::task;

pub(crate) struct Pop<'a> {
    len: usize,
    synced: &'a mut Synced,
}

impl<'a> Pop<'a> {
    pub(super) fn new(len: usize, synced: &'a mut Synced) -> Pop<'a> {
        Pop { len, synced }
    }
}

impl<'a> Iterator for Pop<'a> {
    type Item = task::Notified;

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

impl<'a> ExactSizeIterator for Pop<'a> {
    fn len(&self) -> usize {
        self.len
    }
}

impl<'a> Drop for Pop<'a> {
    fn drop(&mut self) {
        for _ in self.by_ref() {}
    }
}
