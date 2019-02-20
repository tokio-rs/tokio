extern crate futures;
extern crate tokio_mock_task;
extern crate tokio_sync;

use futures::task::{self, Task};
use tokio_mock_task::*;
use tokio_sync::task::AtomicTask;

trait AssertSend: Send {}
trait AssertSync: Send {}

impl AssertSend for AtomicTask {}
impl AssertSync for AtomicTask {}

impl AssertSend for Task {}
impl AssertSync for Task {}

#[test]
fn register_task() {
    // AtomicTask::register_task should *always* register the
    // arbitrary task.

    let atomic = AtomicTask::new();

    let mut mock1 = MockTask::new();
    let mut mock2 = MockTask::new();

    // Register once...
    mock1.enter(|| atomic.register());

    // Grab the actual 2nd task from the mock...
    let task2 = mock2.enter(task::current);

    // Now register the 2nd task, even though in the context where
    // the first task would be considered 'current'...
    {
        // Need a block to grab a reference, so that we only move
        // task2 into the closure, not the AtomicTask...
        let atomic = &atomic;
        mock1.enter(move || {
            atomic.register_task(task2);
        });
    }

    // Just proving that they haven't been notified yet...
    assert!(!mock1.is_notified(), "mock1 shouldn't be notified yet");
    assert!(!mock2.is_notified(), "mock2 shouldn't be notified yet");

    // Now trigger the notify, and ensure it was task2
    atomic.notify();

    assert!(!mock1.is_notified(), "mock1 shouldn't be notified");
    assert!(mock2.is_notified(), "mock2 should be notified");
}
