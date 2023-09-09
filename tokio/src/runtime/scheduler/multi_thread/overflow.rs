use crate::runtime::task;

#[cfg(test)]
use std::cell::RefCell;

pub(crate) trait Overflow {
    fn push(&self, task: task::Notified);

    fn push_batch<I>(&self, iter: I)
    where
        I: Iterator<Item = task::Notified>;
}

#[cfg(test)]
impl Overflow for RefCell<Vec<task::Notified>> {
    fn push(&self, task: task::Notified) {
        self.borrow_mut().push(task);
    }

    fn push_batch<I>(&self, iter: I)
    where
        I: Iterator<Item = task::Notified>,
    {
        self.borrow_mut().extend(iter);
    }
}
