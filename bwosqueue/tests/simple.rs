//! Simple, single threaded test cases

#[cfg(not(loom))]
#[test]
fn simple_enqueue_dequeue() {
    const NB: usize = 8;
    const NE: usize = 1024;
    let (mut owner, _) = bwosqueue::new::<u64, NB, NE>();

    let mut i = 0;
    while owner.enqueue(i).is_ok() {
        i += 1;
    }

    i = 0;
    while let Some(val) = owner.dequeue() {
        assert_eq!(val, i);
        i += 1;
    }
    // use owner outside of iter to control drop
    #[cfg(feature = "stats")]
    assert!(!owner.can_consume())
}

#[cfg(not(loom))]
#[test]
fn steal_block() {
    const NB: usize = 8;
    const NE: usize = 1024;
    let (mut owner, stealer) = bwosqueue::new::<u64, NB, NE>();
    let (mut dst_owner, _) = bwosqueue::new::<u64, NB, NE>();

    let mut i = 0;
    while owner.enqueue(i).is_ok() {
        i += 1;
    }
    // steal all blocks except the consumer block
    for _ in 0..NB - 1 {
        let items = stealer.steal_block().unwrap();
        unsafe { dst_owner.enqueue_batch_unchecked(Box::new(items)) };
    }

    i = 0;
    while let Some(val) = owner.dequeue() {
        assert_eq!(val, i);
        i += 1;
    }

    #[cfg(feature = "stats")]
    assert!(!owner.can_consume());

    while let Some(val) = dst_owner.dequeue() {
        assert_eq!(val, i);
        i += 1;
    }
    #[cfg(feature = "stats")]
    assert!(!dst_owner.can_consume());
}
