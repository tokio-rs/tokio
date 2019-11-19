use super::super::super::test_util;
use super::*;
use loom::cell::CausalCell;
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
