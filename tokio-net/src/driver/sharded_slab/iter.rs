use super::{cfg, page, Shard};
use std::slice;

pub(crate) struct UniqueIter<'a, T, C: cfg::Config> {
    pub(super) shards: slice::IterMut<'a, Shard<T, C>>,
    pub(super) pages: slice::Iter<'a, page::Shared<T, C>>,
    pub(super) slots: Option<page::Iter<'a, T, C>>,
}

impl<'a, T, C: cfg::Config> Iterator for UniqueIter<'a, T, C> {
    type Item = &'a T;
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
