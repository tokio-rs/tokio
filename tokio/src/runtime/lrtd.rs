//! Utility to help with "really nice to add a warning for tasks that might be blocking"
use libc;
use std::collections::HashSet;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{env, thread};

use crate::runtime::{Builder, Runtime};
use crate::util::rand::FastRand;

const PANIC_WORKER_BLOCK_DURATION_DEFAULT: Duration = Duration::from_secs(60);

fn get_panic_worker_block_duration() -> Duration {
    let duration_str = env::var("MY_DURATION_ENV").unwrap_or_else(|_| "60".to_string());
    duration_str
        .parse::<u64>()
        .map(Duration::from_secs)
        .unwrap_or(PANIC_WORKER_BLOCK_DURATION_DEFAULT)
}

fn get_thread_id() -> libc::pthread_t {
    unsafe { libc::pthread_self() }
}

/// A trait for handling actions when blocking is detected.
///
/// This trait provides a method for handling the detection of a blocking action.
pub trait BlockingActionHandler: Send + Sync {
    /// Called when a blocking action is detected and prior to thread signaling.
    ///
    /// # Arguments
    ///
    /// * `workers` - The list of thread IDs of the tokio runtime worker threads.   /// # Returns
    ///
    fn blocking_detected(&self, workers: &[libc::pthread_t]);
}

struct StdErrBlockingActionHandler;

/// BlockingActionHandler implementation that writes blocker details to standard error.
impl BlockingActionHandler for StdErrBlockingActionHandler {
    fn blocking_detected(&self, workers: &[libc::pthread_t]) {
        eprintln!("Detected blocking in worker threads: {:?}", workers);
    }
}

#[derive(Debug)]
struct WorkerSet {
    inner: Mutex<HashSet<libc::pthread_t>>,
}

impl WorkerSet {
    fn new() -> Self {
        WorkerSet {
            inner: Mutex::new(HashSet::new()),
        }
    }

    fn add(&self, pid: libc::pthread_t) {
        let mut set = self.inner.lock().unwrap();
        set.insert(pid);
    }

    fn remove(&self, pid: libc::pthread_t) {
        let mut set = self.inner.lock().unwrap();
        set.remove(&pid);
    }

    fn get_all(&self) -> Vec<libc::pthread_t> {
        let set = self.inner.lock().unwrap();
        set.iter().cloned().collect()
    }
}

/// Utility to help with "really nice to add a warning for tasks that might be blocking"
#[derive(Debug)]
pub struct LongRunningTaskDetector {
    interval: Duration,
    detection_time: Duration,
    stop_flag: Arc<Mutex<bool>>,
    workers: Arc<WorkerSet>,
}

async fn do_nothing(tx: mpsc::Sender<()>) {
    // signal I am done
    tx.send(()).unwrap();
}

fn probe(
    tokio_runtime: &Arc<Runtime>,
    detection_time: Duration,
    workers: &Arc<WorkerSet>,
    action: &Arc<dyn BlockingActionHandler>,
) {
    let (tx, rx) = mpsc::channel();
    let _nothing_handle = tokio_runtime.spawn(do_nothing(tx));
    let is_probe_success = match rx.recv_timeout(detection_time) {
        Ok(_result) => true,
        Err(_) => false,
    };
    if !is_probe_success {
        let targets = workers.get_all();
        action.blocking_detected(&targets);
        rx.recv_timeout(get_panic_worker_block_duration()).unwrap();
    }
}

/// Utility to help with "really nice to add a warning for tasks that might be blocking"
/// Example use:
///  ```
///    use std::sync::Arc;
///    use tokio::runtime::lrtd::LongRunningTaskDetector;
///
///    let mut builder = tokio::runtime::Builder::new_multi_thread();
///    let mutable_builder = builder.worker_threads(2);
///    let lrtd = LongRunningTaskDetector::new(
///      std::time::Duration::from_millis(10),
///      std::time::Duration::from_millis(100),
///      mutable_builder,
///    );
///    let runtime = builder.enable_all().build().unwrap();
///    let arc_runtime = Arc::new(runtime);
///    let arc_runtime2 = arc_runtime.clone();
///    lrtd.start(arc_runtime);
///    arc_runtime2.block_on(async {
///     print!("my async code")
///    });
///
/// ```
///
///    The above will allow you to get details on what is blocking your tokio worker threads for longer that 100ms.
///    The detail will look like:
///    
///  ```text
///     Detected blocking in worker threads: [123145318232064, 123145320341504]
///  ```
///   
///  To get more details(like stack traces) start LongRunningTaskDetector with start_with_custom_action and provide a custom handler.
///
impl LongRunningTaskDetector {
    /// Creates a new `LongRunningTaskDetector` instance.
    ///
    /// # Arguments
    ///
    /// * `interval` - The interval between probes. This interval is randomized.
    /// * `detection_time` - The maximum time allowed for a probe to succeed.
    ///                      A probe running for longer indicates something is blocking the worker threads.
    /// * `runtime_builder` - A mutable reference to a `tokio::runtime::Builder`.
    ///
    /// # Returns
    ///
    /// Returns a new `LongRunningTaskDetector` instance.    
    pub fn new(
        interval: Duration,
        detection_time: Duration,
        runtime_builder: &mut Builder,
    ) -> Self {
        let workers = Arc::new(WorkerSet::new());
        if runtime_builder.is_current_threaded() {
            workers.add(get_thread_id());
        } else {
            let workers_clone = Arc::clone(&workers);
            let workers_clone2 = Arc::clone(&workers);
            runtime_builder
                .on_thread_start(move || {
                    let pid = get_thread_id();
                    workers_clone.add(pid);
                })
                .on_thread_stop(move || {
                    let pid = get_thread_id();
                    workers_clone2.remove(pid);
                });
        }
        LongRunningTaskDetector {
            interval,
            detection_time,
            stop_flag: Arc::new(Mutex::new(true)),
            workers,
        }
    }

    /// Starts the monitoring thread with default action handlers (that write details to std err).
    ///
    /// # Arguments
    ///
    /// * `runtime` - An `Arc` reference to a `tokio::runtime::Runtime`.    
    pub fn start(&self, runtime: Arc<Runtime>) {
        self.start_with_custom_action(runtime, Arc::new(StdErrBlockingActionHandler))
    }

    /// Starts the monitoring process with custom action handlers that
    ///  allow you to customize what happens when blocking is detected.
    ///
    /// # Arguments
    ///
    /// * `runtime` - An `Arc` reference to a `tokio::runtime::Runtime`.
    /// * `action` - An `Arc` reference to a custom `BlockingActionHandler`.
    /// * `thread_action` - An `Arc` reference to a custom `ThreadStateHandler`.
    pub fn start_with_custom_action(
        &self,
        runtime: Arc<Runtime>,
        action: Arc<dyn BlockingActionHandler>,
    ) {
        *self.stop_flag.lock().unwrap() = false;
        let stop_flag = Arc::clone(&self.stop_flag);
        let detection_time = self.detection_time;
        let interval = self.interval;
        let workers = Arc::clone(&self.workers);
        thread::spawn(move || {
            let mut rnd = FastRand::new();
            let max: u32 = <u128 as TryInto<u32>>::try_into(interval.as_micros()).unwrap() - 10;
            while !*stop_flag.lock().unwrap() {
                probe(&runtime, detection_time, &workers, &action);
                thread::sleep(Duration::from_micros(rnd.fastrand_n(max).into()));
            }
        });
    }

    /// Stops the monitoring thread. Does nothing if LRTD is already stopped.
    pub fn stop(&self) {
        let mut sf = self.stop_flag.lock().unwrap();
        if !(*sf) {
            *sf = true;
        }
    }
}

impl Drop for LongRunningTaskDetector {
    fn drop(&mut self) {
        self.stop();
    }
}
