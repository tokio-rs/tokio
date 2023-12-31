use std::backtrace::Backtrace;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::runtime::lrtd::{BlockingActionHandler, LongRunningTaskDetector};

async fn run_blocking_stuff() {
    println!("slow start");
    thread::sleep(Duration::from_secs(1));
    println!("slow done");
}

#[test]
fn test_blocking_detection_multi() {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    let mutable_builder = builder.worker_threads(2);
    let lrtd = LongRunningTaskDetector::new(
        Duration::from_millis(10),
        Duration::from_millis(100),
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
    let mut builder = tokio::runtime::Builder::new_current_thread();
    let mutable_builder = builder.enable_all();
    let lrtd = LongRunningTaskDetector::new(
        Duration::from_millis(10),
        Duration::from_millis(100),
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

#[test]
fn test_blocking_detection_stop_unstarted() {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    let mutable_builder = builder.worker_threads(2);
    let _lrtd = LongRunningTaskDetector::new(
        Duration::from_millis(10),
        Duration::from_millis(100),
        mutable_builder,
    );
}

fn get_thread_id() -> libc::pthread_t {
    unsafe { libc::pthread_self() }
}

static SIGNAL_COUNTER: AtomicUsize = AtomicUsize::new(0);

static THREAD_DUMPS: Mutex<Option<HashMap<libc::pthread_t, String>>> = Mutex::new(None);

extern "C" fn signal_handler(_: i32) {
    // not signal safe, this needs to be rewritten to avoid mem allocations and use a pre-allocated buffer.
    let backtrace = Backtrace::force_capture();
    let name = thread::current()
        .name()
        .map(|n| format!(" for thread \"{}\"", n))
        .unwrap_or("".to_owned());
    let tid = get_thread_id();
    let detail = format!("Stack trace{}:{}\n{}", name, tid, backtrace);
    let mut omap = THREAD_DUMPS.lock().unwrap();
    let map = omap.as_mut().unwrap();
    (*map).insert(tid, detail);
    SIGNAL_COUNTER.fetch_sub(1, Ordering::SeqCst);
}

fn install_thread_stack_stace_handler(signal: libc::c_int) {
    unsafe {
        libc::signal(signal, signal_handler as libc::sighandler_t);
    }
}

static GTI_MUTEX: Mutex<()> = Mutex::new(());

/// A naive stack trace capture implementation for threads for DEMO/TEST only purposes.
fn get_thread_info(
    signal: libc::c_int,
    targets: &[libc::pthread_t],
) -> HashMap<libc::pthread_t, String> {
    let _lock = GTI_MUTEX.lock();
    {
        let mut omap = THREAD_DUMPS.lock().unwrap();
        *omap = Some(HashMap::new());
        SIGNAL_COUNTER.store(targets.len(), Ordering::SeqCst);
    }
    for thread_id in targets {
        let result = unsafe { libc::pthread_kill(*thread_id, signal) };
        if result != 0 {
            eprintln!("Error sending signal: {:?}", result);
        }
    }
    let time_limit = Duration::from_secs(1);
    let start_time = Instant::now();
    loop {
        let signal_count = SIGNAL_COUNTER.load(Ordering::SeqCst);
        if signal_count <= 0 {
            SIGNAL_COUNTER.store(0, Ordering::SeqCst);
            break;
        }
        if Instant::now() - start_time >= time_limit {
            break;
        }
        std::thread::sleep(std::time::Duration::from_micros(10));
    }
    {
        let omap = THREAD_DUMPS.lock().unwrap();
        omap.clone().unwrap().clone()
    }
}

struct DetailedCaptureBlockingActionHandler {
    inner: Mutex<Option<HashMap<libc::pthread_t, String>>>,
}

impl DetailedCaptureBlockingActionHandler {
    fn new() -> Self {
        DetailedCaptureBlockingActionHandler {
            inner: Mutex::new(None),
        }
    }

    fn contains_symbol(&self, symbol_name: &str) -> bool {
        // Iterate over the frames in the backtrace
        let omap = self.inner.lock().unwrap();
        match omap.as_ref() {
            Some(map) => {
                if map.is_empty() {
                    false
                } else {
                    let bt_str = map.values().next().unwrap();
                    bt_str.contains(symbol_name)
                }
            }
            None => false,
        }
    }
}

impl BlockingActionHandler for DetailedCaptureBlockingActionHandler {
    fn blocking_detected(&self, workers: &[libc::pthread_t]) {
        let mut map = self.inner.lock().unwrap();
        let tinfo = get_thread_info(libc::SIGUSR1, workers);
        eprint!("Blocking detected with details: {:?}\n", tinfo);
        *map = Some(tinfo);
    }
}

#[test]
fn test_blocking_detection_multi_capture() {
    install_thread_stack_stace_handler(libc::SIGUSR1);
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    let mutable_builder = builder.worker_threads(2);
    let lrtd = LongRunningTaskDetector::new(
        Duration::from_millis(10),
        Duration::from_millis(100),
        mutable_builder,
    );
    let runtime = builder.enable_all().build().unwrap();
    let arc_runtime = Arc::new(runtime);
    let arc_runtime2 = arc_runtime.clone();
    let blocking_action = Arc::new(DetailedCaptureBlockingActionHandler::new());
    let to_assert_blocking = blocking_action.clone();
    lrtd.start_with_custom_action(arc_runtime, blocking_action);
    arc_runtime2.spawn(run_blocking_stuff());
    arc_runtime2.spawn(run_blocking_stuff());
    arc_runtime2.block_on(async {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        println!("Hello world");
    });
    assert!(to_assert_blocking.contains_symbol("std::thread::sleep"));
    lrtd.stop()
}
