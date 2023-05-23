use crate::runtime::task::Inject;

#[test]
fn push_and_pop() {
    let inject = Inject::new();

    for _ in 0..10 {
        let (task, _) = super::unowned(async {});
        inject.push(task);
    }

    for _ in 0..10 {
        assert!(inject.pop().is_some());
    }

    assert!(inject.pop().is_none());
}

#[test]
fn push_batch_and_pop() {
    let inject = Inject::new();

    inject.push_batch((0..10).map(|_| super::unowned(async {}).0));

    assert_eq!(5, inject.pop_n(5).count());
    assert_eq!(5, inject.pop_n(5).count());
    assert_eq!(0, inject.pop_n(5).count());
}

#[test]
fn pop_n_drains_on_drop() {
    let inject = Inject::new();

    inject.push_batch((0..10).map(|_| super::unowned(async {}).0));
    let _ = inject.pop_n(10);

    assert_eq!(inject.len(), 0);
}
