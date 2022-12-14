use super::AtomicU64;
use crate::loom::sync::{atomic::Ordering, Mutex};
use crate::util::once_cell::OnceCell;

pub(crate) struct StaticAtomicU64 {
    init: u64,
    cell: OnceCell<Mutex<u64>>,
}

impl AtomicU64 {
    pub(crate) fn new(val: u64) -> Self {
        Self {
            inner: Mutex::new(val),
        }
    }
}

impl StaticAtomicU64 {
    pub(crate) const fn new(val: u64) -> StaticAtomicU64 {
        StaticAtomicU64 {
            init: val,
            cell: OnceCell::new(),
        }
    }

    pub(crate) fn fetch_add(&self, val: u64, order: Ordering) -> u64 {
        let mut lock = self.inner().lock();
        let prev = *lock;
        *lock = prev + val;
        prev
    }

    fn inner(&self) -> &Mutex<u64> {
        self.cell.get(|| Mutex::new(self.init))
    }
}
