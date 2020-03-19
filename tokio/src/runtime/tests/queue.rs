use crate::runtime::queue;
use crate::runtime::task::{self, Schedule, Task};

#[test]
fn steal_batch() {
    let (steal1, mut local1) = queue::local();
    let (_, mut local2) = queue::local();
    let inject = queue::Inject::new();

    for _ in 0..4 {
        let (task, _) = task::joinable::<_, Runtime>(async {});
        local1.push_back(task, &inject);
    }

    assert!(steal1.steal_into(&mut local2).is_some());

    for _ in 0..1 {
        assert!(local2.pop().is_some());
    }

    assert!(local2.pop().is_none());

    for _ in 0..2 {
        assert!(local1.pop().is_some());
    }

    assert!(local1.pop().is_none());
}

struct Runtime;

impl Schedule for Runtime {
    fn bind(task: Task<Self>) -> Runtime {
        std::mem::forget(task);
        Runtime
    }

    fn release(&self, _task: &Task<Self>) -> Option<Task<Self>> {
        None
    }

    fn schedule(&self, _task: task::Notified<Self>) {
        unreachable!();
    }
}
