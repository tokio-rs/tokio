use super::{page, ScheduledIo, Shard};
use std::slice;

pub(in crate::driver::reactor) struct UniqueIter<'a> {
    pub(super) shards: slice::IterMut<'a, Shard>,
    pub(super) pages: slice::Iter<'a, page::Shared>,
    pub(super) slots: Option<page::Iter<'a>>,
}

impl<'a> Iterator for UniqueIter<'a> {
    type Item = &'a ScheduledIo;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(item) = self.slots.as_mut().and_then(|slots| slots.next()) {
                return Some(item);
            }

            if let Some(page) = self.pages.next() {
                self.slots = page.iter();
            }

            if let Some(shard) = self.shards.next() {
                self.pages = shard.iter();
            } else {
                return None;
            }
        }
    }
}
