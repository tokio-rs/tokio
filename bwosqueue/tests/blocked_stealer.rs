#![cfg(not(loom))]
// A test to check behaviour is sane when the queue is full and empty at the same time
// when waiting on a stealer.
#[test]
fn blocked_stealer() {
    const NUM_BLOCKS: usize = 4;
    const ENTRIES_PER_BLOCK: usize = 4;
    let (mut owner, stealer) = bwosqueue::new::<usize, NUM_BLOCKS, ENTRIES_PER_BLOCK>();
    let mut total_enqueues = 0;
    for i in 0..ENTRIES_PER_BLOCK + 2 {
        owner.enqueue(i).unwrap();
        total_enqueues += 1;
    }
    let mut stolen_iter = stealer.steal_block().unwrap();
    // We have now reserved the items in the block but not dequeued them yet.

    let mut total_dequeued = 0;
    // consume first block
    while let Some(val) = owner.dequeue() {
        assert_eq!(val, total_dequeued);
        total_dequeued += 1;
    }

    // push until full
    while owner.enqueue(total_enqueues).is_ok() {
        total_enqueues += 1;
    }

    while let Some(val) = owner.dequeue() {
        // 2 entries where already reserved by stealer
        assert_eq!(val, total_dequeued + 2);
        total_dequeued += 1;
    }

    // We wrapped around once and are now stuck at the end of the first block waiting on the stealer
    assert_eq!(total_enqueues, (NUM_BLOCKS + 1) * ENTRIES_PER_BLOCK);
    assert_eq!(total_dequeued, (NUM_BLOCKS + 1) * ENTRIES_PER_BLOCK - 2);

    assert_eq!(
        ENTRIES_PER_BLOCK,
        stolen_iter.next().expect("No stolen item")
    );
    // Stealer is not finished yet, so consumer and producer should still be stuck
    assert_eq!(owner.enqueue(42), Err(42));
    assert_eq!(owner.dequeue(), None);
    // Let the stealer finish
    assert_eq!(
        ENTRIES_PER_BLOCK + 1,
        stolen_iter.next().expect("No stolen item")
    );
    assert_eq!(stolen_iter.next(), None);
    // Manually drop to unstuck
    drop(stolen_iter);
    // Producer and Consumer are now both unstuck, but the queue is still empty.
    assert_eq!(owner.dequeue(), None);
    assert!(owner.enqueue(1).is_ok());
    assert_eq!(total_enqueues, total_dequeued + 2);
}
