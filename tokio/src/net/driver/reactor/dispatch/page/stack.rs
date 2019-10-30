use crate::loom::atomic::{AtomicUsize, Ordering};
use std::fmt;

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
        if val == super::Addr::NULL {
            None
        } else {
            Some(val)
        }
    }

    pub(super) fn push(&self, value: usize, before: impl Fn(usize)) {
        let mut next = self.head.load(Ordering::Relaxed);
        loop {
            before(next);

            match self
                .head
                .compare_exchange(next, value, Ordering::AcqRel, Ordering::Acquire)
            {
                // lost the race!
                Err(actual) => next = actual,
                Ok(_) => return,
            }
        }
    }
}

impl fmt::Debug for TransferStack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Loom likes to dump all its internal state in `fmt::Debug` impls, so
        // we override this to just print the current value in tests.
        f.debug_struct("TransferStack")
            .field(
                "head",
                &format_args!("{:#x}", self.head.load(Ordering::Relaxed)),
            )
            .finish()
    }
}

#[cfg(all(test, loom))]
mod test {
    use super::super::super::test_util;
    use super::*;
    use crate::loom::CausalCell;
    use loom::thread;
    use std::sync::Arc;

    #[test]
    fn transfer_stack() {
        test_util::run_model("transfer_stack", || {
            let causalities = [CausalCell::new(None), CausalCell::new(None)];
            let shared = Arc::new((causalities, TransferStack::new()));
            let shared1 = shared.clone();
            let shared2 = shared.clone();

            // Spawn two threads that both try to push to the stack.
            let t1 = thread::spawn(move || {
                let (causalities, stack) = &*shared1;
                stack.push(0, |prev| {
                    causalities[0].with_mut(|c| unsafe {
                        *c = Some(prev);
                    });
                    test_println!("prev={:#x}", prev)
                });
            });

            let t2 = thread::spawn(move || {
                let (causalities, stack) = &*shared2;
                stack.push(1, |prev| {
                    causalities[1].with_mut(|c| unsafe {
                        *c = Some(prev);
                    });
                    test_println!("prev={:#x}", prev)
                });
            });

            let (causalities, stack) = &*shared;

            // Try to pop from the stack...
            let mut idx = stack.pop_all();
            while idx == None {
                idx = stack.pop_all();
                thread::yield_now();
            }
            let idx = idx.unwrap();
            test_println!("popped {:#x}", idx);

            let saw_both = causalities[idx].with(|val| {
                let val = unsafe { *val };
                assert!(
                    val.is_some(),
                    "CausalCell write must happen-before index is pushed to the stack!",
                );
                // were there two entries in the stack? if so, check that
                // both saw a write.
                if let Some(c) = causalities.get(val.unwrap()) {
                    test_println!("saw both entries!");
                    c.with(|val| {
                        let val = unsafe { *val };
                        assert!(
                            val.is_some(),
                            "CausalCell write must happen-before index is pushed to the stack!",
                        );
                    });
                    true
                } else {
                    false
                }
            });

            // We only saw one push. Ensure that the other push happens too.
            if !saw_both {
                // Try to pop from the stack...
                let mut idx = stack.pop_all();
                while idx == None {
                    idx = stack.pop_all();
                    thread::yield_now();
                }
                let idx = idx.unwrap();

                test_println!("popped {:#x}", idx);
                causalities[idx].with(|val| {
                    let val = unsafe { *val };
                    assert!(
                        val.is_some(),
                        "CausalCell write must happen-before index is pushed to the stack!",
                    );
                });
            }

            t1.join().unwrap();
            t2.join().unwrap();
        });
    }
}
