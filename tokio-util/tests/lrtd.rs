#![cfg(all(feature = "lrtd"))]
use std::backtrace::Backtrace;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio_util::lrtd::{BlockingActionHandler, LongRunningTaskDetector, ThreadStateHandler};

async fn run_blocking_stuff() {
    println!("slow start");
    thread::sleep(Duration::from_secs(1));
    println!("slow done");
}

static GLOBAL_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_blocking_detection_multi() {
    let _guard = GLOBAL_MUTEX.lock().unwrap();
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    let mutable_builder = builder.worker_threads(2);
    let lrtd = LongRunningTaskDetector::new(
        Duration::from_millis(10),
        Duration::from_millis(100),
        libc::SIGUSR1,
        mutable_builder,
    );
    let runtime = builder.enable_all().build().unwrap();
    let arc_runtime = Arc::new(runtime);
    let arc_runtime2 = arc_runtime.clone();
    lrtd.start(arc_runtime);
    arc_runtime2.spawn(run_blocking_stuff());
    arc_runtime2.spawn(run_blocking_stuff());
    arc_runtime2.block_on(async {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        println!("Done");
    });
}

#[test]
fn test_blocking_detection_current() {
    let _guard = GLOBAL_MUTEX.lock().unwrap();
    let mut builder = tokio::runtime::Builder::new_current_thread();
    let mutable_builder = builder.enable_all();
    let lrtd = LongRunningTaskDetector::new(
        Duration::from_millis(10),
        Duration::from_millis(100),
        libc::SIGUSR1,
        mutable_builder,
    );
    let runtime = mutable_builder.build().unwrap();
    let arc_runtime = Arc::new(runtime);
    let arc_runtime2 = arc_runtime.clone();
    lrtd.start(arc_runtime);
    arc_runtime2.block_on(async {
        run_blocking_stuff().await;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        println!("Done");
    });
}

struct CaptureBlockingActionHandler {
    inner: Mutex<HashSet<libc::pthread_t>>,
}

impl CaptureBlockingActionHandler {
    fn new() -> Self {
        CaptureBlockingActionHandler {
            inner: Mutex::new(HashSet::new()),
        }
    }

    fn get_all(&self) -> Vec<libc::pthread_t> {
        let set = self.inner.lock().unwrap();
        set.iter().cloned().collect()
    }
}

impl BlockingActionHandler for CaptureBlockingActionHandler {
    fn blocking_detected(&self, _signal: libc::c_int, targets: &Vec<libc::pthread_t>) -> bool {
        let mut set = self.inner.lock().unwrap();
        set.extend(targets);
        true
    }
}

struct CaptureThreadStateHandler {
    inner: Mutex<Vec<String>>,
}

impl CaptureThreadStateHandler {
    fn new() -> Self {
        CaptureThreadStateHandler {
            inner: Mutex::new(Vec::new()),
        }
    }

    fn get_all(&self) -> Vec<String> {
        let vec = self.inner.lock().unwrap();
        vec.clone()
    }

    fn contains_symbol(&self, symbol_name: &str) -> bool {
        // Iterate over the frames in the backtrace
        let traces = self.get_all();
        if traces.is_empty() {
            return false;
        }
        let bt_str = traces.get(0).unwrap();
        bt_str.contains(symbol_name)
    }
}

impl ThreadStateHandler for CaptureThreadStateHandler {
    fn blocking_thread_details(
        &self,
        _thread_id: libc::pthread_t,
        _thread_name: Option<&str>,
        backtrace: Backtrace,
    ) {
        let mut vec = self.inner.lock().unwrap();
        vec.push(format!("{}", backtrace));
    }
}

#[test]
fn test_blocking_detection_multi_capture() {
    let _guard = GLOBAL_MUTEX.lock().unwrap();
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    let mutable_builder = builder.worker_threads(2);
    let lrtd = LongRunningTaskDetector::new(
        Duration::from_millis(10),
        Duration::from_millis(100),
        libc::SIGUSR1,
        mutable_builder,
    );
    let runtime = builder.enable_all().build().unwrap();
    let arc_runtime = Arc::new(runtime);
    let arc_runtime2 = arc_runtime.clone();
    let blocking_action = Arc::new(CaptureBlockingActionHandler::new());
    let thread_state_action = Arc::new(CaptureThreadStateHandler::new());
    let to_assert_blocking = blocking_action.clone();
    let to_assert_thread = thread_state_action.clone();
    lrtd.start_with_custom_action(arc_runtime, blocking_action, thread_state_action);
    arc_runtime2.spawn(run_blocking_stuff());
    arc_runtime2.spawn(run_blocking_stuff());
    arc_runtime2.block_on(async {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        println!("Hello world");
    });
    assert_eq!(2, to_assert_blocking.get_all().len());
    assert!(to_assert_thread.contains_symbol("std::thread::sleep"));
    lrtd.stop()
}

#[test]
fn test_blocking_detection_stop_unstarted() {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    let mutable_builder = builder.worker_threads(2);
    let _lrtd = LongRunningTaskDetector::new(
        Duration::from_millis(10),
        Duration::from_millis(100),
        libc::SIGUSR1,
        mutable_builder,
    );
}
