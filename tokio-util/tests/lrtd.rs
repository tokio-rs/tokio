use nix::sys::signal::Signal;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio_util::lrtd::LongRunningTaskDetector;

async fn run_blocking_stuff() {
    println!("slow start");
    thread::sleep(Duration::from_secs(2));
    println!("slow done");
}

#[test]
fn test_blocking_detection_multi() {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    let mutable_builder = builder.worker_threads(2);
    let lrtd = LongRunningTaskDetector::new(
        Duration::from_millis(10),
        Duration::from_millis(100),
        Signal::SIGUSR1,
        mutable_builder,
    );
    let runtime = builder.enable_all().build().unwrap();
    let arc_runtime = Arc::new(runtime);
    let arc_runtime2 = arc_runtime.clone();
    lrtd.start(arc_runtime.clone());
    arc_runtime2.spawn(run_blocking_stuff());
    arc_runtime2.spawn(run_blocking_stuff());
    arc_runtime2.block_on(async {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        println!("Hello world");
    });
    lrtd.stop()
}

#[test]
fn test_blocking_detection_current() {
    let mut builder = tokio::runtime::Builder::new_current_thread();
    let mutable_builder = builder.enable_all();
    let lrtd = LongRunningTaskDetector::new(
        Duration::from_millis(10),
        Duration::from_millis(100),
        Signal::SIGUSR1,
        mutable_builder,
    );
    let runtime = mutable_builder.build().unwrap();
    let arc_runtime = Arc::new(runtime);
    let arc_runtime2 = arc_runtime.clone();
    lrtd.start(arc_runtime);
    arc_runtime2.block_on(async {
        run_blocking_stuff().await;
        println!("Sleeping");
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        println!("Hello world");
    });
    lrtd.stop()
}
