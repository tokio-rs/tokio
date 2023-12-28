use nix::sys::signal::Signal;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio_util::lrtd::{
    install_thread_stack_stace_handler, LongRunningTaskDetector,
};

async fn run_blocking_stuff() {
    println!("slow start");
    thread::sleep(Duration::from_secs(2));
    println!("slow done");
}

fn install_thread_stack_stace_handler_default() {
    install_thread_stack_stace_handler(Signal::SIGUSR1);
}

#[test]
fn test_blocking_detection() {
    install_thread_stack_stace_handler_default();
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
    lrtd.start(arc_runtime.clone());
    let runtime_handle = arc_runtime.handle();
    runtime_handle.spawn(run_blocking_stuff());
    runtime_handle.spawn(run_blocking_stuff());    
    runtime_handle.block_on(async {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        println!("Hello world");
    });
    lrtd.stop()
}
