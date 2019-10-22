use crate::sync::atomic::{spin_loop_hint, AtomicUsize, Ordering};

#[derive(Debug)]
pub(super) struct TransferStack {
    head: AtomicUsize,
}

impl TransferStack {
    pub(super) fn new() -> Self {
        Self {
            head: AtomicUsize::new(super::Addr::NULL),
        }
    }

    pub(super) fn pop_all(&self) -> Option<usize> {
        let val = self.head.swap(super::Addr::NULL, Ordering::Acquire);
        test_println!("-> pop_all; head={:#x}", val);
        if val == super::Addr::NULL {
            None
        } else {
            Some(val)
        }
    }

    pub(super) fn push(&self, value: usize, before: impl Fn(usize)) {
        let mut next = self.head.load(Ordering::Relaxed);
        loop {
            test_println!("-> next {:#x}", next);
            before(next);

            match self
                .head
                .compare_exchange(next, value, Ordering::Release, Ordering::Relaxed)
            {
                // lost the race!
                Err(actual) => {
                    test_println!("-> retry!");
                    next = actual;
                }
                Ok(_) => {
                    test_println!("-> successful; next={:#x}", next);
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::sync::CausalCell;
    use loom::thread;
    use std::sync::Arc;

    #[test]
    fn transfer_stack() {
        loom::model(|| {
            println!("\n ----- iter ------ \n");
            let causalities = [CausalCell::new(999), CausalCell::new(999)];
            let shared = Arc::new((causalities, TransferStack::new()));
            let shared1 = shared.clone();
            let shared2 = shared.clone();

            let t1 = thread::spawn(move || {
                let (causalities, stack) = &*shared1;
                stack.push(0, |prev| {
                    causalities[0].with_mut(|c| unsafe {
                        *c = 0;
                    });
                    test_println!("prev={:#x}", prev)
                });
            });
            let t2 = thread::spawn(move || {
                let (causalities, stack) = &*shared2;
                stack.push(1, |prev| {
                    causalities[1].with_mut(|c| unsafe {
                        *c = prev;
                    });
                    test_println!("prev={:#x}", prev)
                });
            });

            let (causalities, stack) = &*shared;
            let mut idx = stack.pop_all();
            while idx == None {
                idx = stack.pop_all();
                thread::yield_now();
            }
            let idx = idx.unwrap();
            causalities[idx].with(|val| {
                let val = unsafe { *val };
                assert_ne!(
                    val, 999,
                    "CausalCell write must happen-before index is pushed to the stack!"
                );
                // were there two entries in the stack? if so, check that
                // both saw a write.
                if let Some(c) = causalities.get(val) {
                    c.with(|val| {
                        let val = unsafe { *val };
                        assert_ne!(
                            val, 999,
                            "CausalCell write must happen-before index is pushed to the stack!"
                        );
                    });
                }
            });

            t1.join().unwrap();
            t2.join().unwrap();
        });
    }
}
