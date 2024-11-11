#![allow(unknown_lints, unexpected_cfgs)]
#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", tokio_unstable, target_has_atomic = "64"))]

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::runtime::Builder;

const TASKS: usize = 8;
const ITERATIONS: usize = 64;
/// Assert that the spawn task hook always fires when set.
#[test]
fn spawn_task_hook_fires() {
    let count = Arc::new(AtomicUsize::new(0));
    let count2 = Arc::clone(&count);

    let ids = Arc::new(Mutex::new(HashSet::new()));
    let ids2 = Arc::clone(&ids);

    let runtime = Builder::new_current_thread()
        .on_task_spawn(move |data| {
            ids2.lock().unwrap().insert(data.id());

            count2.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .unwrap();

    for _ in 0..TASKS {
        runtime.spawn(std::future::pending::<()>());
    }

    let count_realized = count.load(Ordering::SeqCst);
    assert_eq!(
        TASKS, count_realized,
        "Total number of spawned task hook invocations was incorrect, expected {TASKS}, got {}",
        count_realized
    );

    let count_ids_realized = ids.lock().unwrap().len();

    assert_eq!(
        TASKS, count_ids_realized,
        "Total number of spawned task hook invocations was incorrect, expected {TASKS}, got {}",
        count_realized
    );
}

/// Assert that the terminate task hook always fires when set.
#[test]
fn terminate_task_hook_fires() {
    let count = Arc::new(AtomicUsize::new(0));
    let count2 = Arc::clone(&count);

    let runtime = Builder::new_current_thread()
        .on_task_terminate(move |_data| {
            count2.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .unwrap();

    for _ in 0..TASKS {
        runtime.spawn(std::future::ready(()));
    }

    runtime.block_on(async {
        // tick the runtime a bunch to close out tasks
        for _ in 0..ITERATIONS {
            tokio::task::yield_now().await;
        }
    });

    assert_eq!(TASKS, count.load(Ordering::SeqCst));
}
