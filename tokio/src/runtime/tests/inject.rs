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
