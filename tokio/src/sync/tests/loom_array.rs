use crate::sync::mpsc::array;

use loom::thread;
use std::sync::Arc;

#[test]
fn reuse_slot() {
    use crate::sync::mpsc::block::Read;

    loom::model(|| {
        let (tx, mut rx) = array::channel(1);
        let tx = Arc::new(tx);

        tx.push(0);

        let tx2 = Arc::clone(&tx);
        let join = thread::spawn(move || {
            // This send wraps around to the same slot. It may start before the
            // receiver has emptied that slot, so this exercises the array
            // queue's slot-level reuse synchronization directly.
            tx2.push(1);
        });

        for expected in 0..2 {
            loop {
                match rx.pop(&tx) {
                    Some(Read::Value(value)) => {
                        assert_eq!(value, expected);
                        break;
                    }
                    Some(Read::Closed) => {
                        panic!();
                    }
                    None => {
                        thread::yield_now();
                    }
                }
            }
        }

        join.join().unwrap();
    });
}

#[test]
fn concurrent_reuse_slots() {
    use crate::sync::mpsc::block::Read;

    const TASKS: usize = 3;

    fn pop(rx: &mut array::Rx<usize>, tx: &array::Tx<usize>) -> usize {
        loop {
            match rx.pop(tx) {
                Some(Read::Value(value)) => return value,
                Some(Read::Closed) => {
                    panic!();
                }
                None => {
                    thread::yield_now();
                }
            }
        }
    }

    loom::model(|| {
        let (tx, rx) = array::channel(2);
        let tx = Arc::new(tx);
        let rx = Arc::new(loom::sync::Mutex::new(rx));

        tx.push(0);
        tx.push(1);

        let mut threads = Vec::new();

        for i in 0..TASKS {
            let tx = Arc::clone(&tx);
            let rx = Arc::clone(&rx);

            threads.push(thread::spawn(move || {
                let _ = pop(&mut rx.lock().unwrap(), &tx);
                thread::yield_now();
                tx.push(i + 2);
            }));
        }

        for thread in threads {
            thread.join().unwrap();
        }
    });
}
