#![cfg(all(feature = "full", tokio_unstable))]

use tokio::task;
use tokio_test::task::spawn;

// `yield_now` is tested within the runtime in `rt_common`.
#[test]
fn yield_now_outside_of_runtime() {
    let mut task = spawn(async {
        task::yield_now().await;
    });

    assert!(task.poll().is_pending());
    assert!(task.is_woken());
    assert!(task.poll().is_ready());
}
