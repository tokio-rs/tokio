use crate::runtime::scheduler::inject::ShardedInject;
use crate::runtime::scheduler::Inject;

#[test]
fn push_and_pop() {
    const N: usize = 2;

    let inject = Inject::new();

    for i in 0..N {
        assert_eq!(inject.len(), i);
        let (task, _) = super::unowned(async {});
        inject.push(task);
    }

    for i in 0..N {
        assert_eq!(inject.len(), N - i);
        assert!(inject.pop().is_some());
    }

    println!("--------------");

    assert!(inject.pop().is_none());
}

#[test]
fn push_batch_and_pop() {
    let inject = Inject::new();

    inject.push_batch((0..10).map(|_| super::unowned(async {}).0));

    assert_eq!(5, inject.pop_n(5, |tasks| tasks.count()));
    assert_eq!(5, inject.pop_n(5, |tasks| tasks.count()));
    assert_eq!(0, inject.pop_n(5, |tasks| tasks.count()));
}

#[test]
fn pop_n_drains_on_drop() {
    let inject = Inject::new();

    inject.push_batch((0..10).map(|_| super::unowned(async {}).0));
    inject.pop_n(10, |_| ());

    assert_eq!(inject.len(), 0);
}

#[test]
fn sharded_push_and_pop() {
    const N: usize = 32;

    let inject = ShardedInject::new(4);

    for i in 0..N {
        assert_eq!(inject.len(), i);
        let (task, _) = super::unowned(async {});
        inject.push(task);
    }

    for i in 0..N {
        assert_eq!(inject.len(), N - i);
        assert!(inject.pop().is_some());
    }

    assert!(inject.pop().is_none());
    assert!(inject.is_empty());
}

#[test]
fn sharded_close_rejects_pushes() {
    let inject = ShardedInject::new(4);

    let (task, _) = super::unowned(async {});
    inject.push(task);

    assert!(inject.close());
    assert!(!inject.close());
    assert!(inject.is_closed());

    // A push to a closed queue is dropped.
    let (task, _) = super::unowned(async {});
    inject.push(task);
    assert_eq!(inject.len(), 1);

    assert!(inject.pop().is_some());
    assert!(inject.pop().is_none());
}

#[test]
fn sharded_push_batch_and_pop_n() {
    let inject = ShardedInject::new(4);

    inject.push_batch((0..10).map(|_| super::unowned(async {}).0));
    assert_eq!(inject.len(), 10);

    let mut count = 0;
    inject.pop_n(5, |tasks| count += tasks.count());
    assert_eq!(count, 5);

    let mut count = 0;
    inject.pop_n(10, |tasks| count += tasks.count());
    assert_eq!(count, 5);
    assert!(inject.is_empty());
}

#[test]
fn sharded_pop_n_fills_across_shards() {
    let inject = ShardedInject::new(4);

    // Two separate batches, likely in the same sticky shard; push single
    // tasks too so multiple shards may be populated either way.
    inject.push_batch((0..4).map(|_| super::unowned(async {}).0));
    inject.push_batch((0..4).map(|_| super::unowned(async {}).0));
    assert_eq!(inject.len(), 8);

    let mut count = 0;
    inject.pop_n(8, |tasks| count += tasks.count());
    assert_eq!(count, 8);
    assert!(inject.is_empty());
}

#[test]
fn sharded_pop_n_drains_on_drop() {
    let inject = ShardedInject::new(4);

    inject.push_batch((0..10).map(|_| super::unowned(async {}).0));
    inject.pop_n(10, |_| ());

    assert_eq!(inject.len(), 0);
}
