use crate::runtime::task;

#[cfg(test)]
use std::cell::RefCell;

pub(crate) trait OverflowShard<T: 'static> {
    fn push(&self, task: task::Notified<T>, group: usize);

    fn push_batch<I>(&self, iter: I, group: usize)
        where I: Iterator<Item = task::Notified<T>>;
}

#[cfg(test)]
impl<T: 'static> OverflowShard<T> for RefCell<Vec<Vec<task::Notified<T>>>> {
    fn push(&self, task: task::Notified<T>, group: usize) {
        self.borrow_mut()[group].push(task);
    }

    fn push_batch<I>(&self, iter: I, group: usize)
    where
        I: Iterator<Item = task::Notified<T>>,
    {
        self.borrow_mut()[group].extend(iter);
    }
}
