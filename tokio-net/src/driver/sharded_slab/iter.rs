use super::{page, Shard};
use std::slice;

pub(crate) struct UniqueIter<'a, T> {
    pub(super) shards: slice::IterMut<'a, Shard<T>>,
    pub(super) pages: slice::Iter<'a, page::Shared<T>>,
    pub(super) slots: Option<page::Iter<'a, T>>,
}

impl<'a, T> Iterator for UniqueIter<'a, T> {
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
