#![cfg(all(
    tokio_unstable,
    tokio_taskdump,
    target_os = "linux",
    any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")
))]

use std::hint::black_box;
use tokio::runtime;

#[inline(never)]
async fn a() {
    black_box(b()).await
}

#[inline(never)]
async fn b() {
    black_box(c()).await
}

#[inline(never)]
async fn c() {
    black_box(tokio::task::yield_now()).await
}

#[test]
fn test() {
    let rt = runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.spawn(a());

    let handle = rt.handle();

    assert_eq!(handle.dump().tasks().iter().count(), 0);

    let dump = rt.block_on(async {
        handle.spawn(a());
        handle.dump()
    });

    let tasks: Vec<_> = dump.tasks().iter().collect();

    assert_eq!(tasks.len(), 2);

    for task in tasks {
        let trace = task.trace().to_string();
        assert!(trace.contains("dump_current_thread::a"));
        assert!(trace.contains("dump_current_thread::b"));
        assert!(trace.contains("dump_current_thread::c"));
        assert!(trace.contains("tokio::task::yield_now"));
    }
}
