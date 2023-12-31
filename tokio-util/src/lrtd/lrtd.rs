/// Utility to help with "really nice to add a warning for tasks that might be blocking"
use libc;
use rand::Rng;
use std::backtrace::Backtrace;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;
use std::{env, thread};
use tokio::runtime::{Builder, Runtime};

static mut GLOBAL_COUNTER: Option<AtomicUsize> = None;
static INIT: Once = Once::new();

fn initialize_global_counter() {
    INIT.call_once(|| unsafe {
        GLOBAL_COUNTER = Some(AtomicUsize::new(0));
    });
}

/// The purpose of this method is to generate unique IDs for LRTD instances.
fn get_next_global_value() -> usize {
    initialize_global_counter();

    unsafe {
        GLOBAL_COUNTER
            .as_ref()
            .expect("Global counter not initialized")
            .fetch_add(1, Ordering::Relaxed)
    }
}

struct OwnedThreadStateHandler {
    owners: Vec<usize>,
    action: Arc<dyn ThreadStateHandler>,
}

impl OwnedThreadStateHandler {
    fn add(&mut self, action: Arc<dyn ThreadStateHandler>, owner: usize) {
        if !Arc::ptr_eq(&action, &(self.action)) {
            panic!("Cannot overwrite action with something different!");
        }
        for x_owner in &(self.owners) {
            if *x_owner == owner {
                panic!(
                    "Cannot set thread state handler twice by the same owner: {}",
                    owner
                );
            }
        }
        self.owners.push(owner);
    }

    fn remove(&mut self, owner: usize) {
        if let Some(index) = self.owners.iter().position(|&x| x == owner) {
            let removed_value = self.owners.remove(index);
            if removed_value != owner {
                panic!("Wrong value, whould have been {}", owner);
            }
        } else {
            panic!("Cannot find owner: {}", owner)
        }
    }
}

static GLOBAL_TH_STATE_HANDLER: Mutex<Option<OwnedThreadStateHandler>> = Mutex::new(None);

fn set_thread_state_handler(action: Arc<dyn ThreadStateHandler>, owner: usize) {
    let mut value = GLOBAL_TH_STATE_HANDLER.lock().unwrap();
    match value.as_mut() {
        Some(handler) => {
            (*handler).add(action, owner);
        }
        None => {
            let mut owners = Vec::new();
            owners.push(owner);
            *value = Some(OwnedThreadStateHandler { owners, action });
        }
    }
}

fn reset_thread_state_handler(owner: usize) {
    let mut value = GLOBAL_TH_STATE_HANDLER.lock().unwrap();
    let h = match value.as_mut() {
        Some(handler) => {
            (*handler).remove(owner);
            handler
        }
        None => {
            panic!(
                "No action handler found for: {}, likely LongRunningTaskDetector not started",
                owner
            )
        }
    };
    if h.owners.is_empty() {
        *value = None
    }
}

fn get_thread_state_handler() -> Arc<dyn ThreadStateHandler> {
    let option = GLOBAL_TH_STATE_HANDLER.lock().unwrap();
    match option.as_ref() {
        Some(value) => value.action.clone(),
        None => Arc::new(StdErrThreadStateHandler),
    }
}

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

/// A trait for handling thread state details.
///
/// This trait provides a method for capturing details of a blocking thread.
pub trait ThreadStateHandler: Send + Sync {
    /// Invoked with details of a blocking thread.
    ///
    /// # Arguments
    ///
    /// * `thread_id` - The ID of the blocking thread.
    /// * `thread_name` - An optional name of the blocking thread.
    /// * `backtrace` - The backtrace of the blocking thread.    
    fn blocking_thread_details(
        &self,
        thread_id: libc::pthread_t,
        thread_name: Option<&str>,
        backtrace: Backtrace,
    );
}

/// A trait for handling actions when blocking is detected.
///
/// This trait provides a method for handling the detection of a blocking action.
pub trait BlockingActionHandler: Send + Sync {
    /// Called when a blocking action is detected and prior to thread signaling.
    ///
    /// # Arguments
    ///
    /// * `signal` - The signal used to signal tokio worker threads for state details.
    /// * `targets` - The list of thread IDs of the tokio runtime worker threads.   /// # Returns
    ///
    /// # Returns
    ///
    /// Returns `true` if the signaling of the threads is to be executed, false otherwise.
    ///
    fn blocking_detected(&self, signal: libc::c_int, targets: &Vec<libc::pthread_t>) -> bool;
}

struct StdErrBlockingActionHandler;

/// BlockingActionHandler implementation that writes blocker details to standard error.
impl BlockingActionHandler for StdErrBlockingActionHandler {
    fn blocking_detected(&self, signal: libc::c_int, targets: &Vec<libc::pthread_t>) -> bool {
        eprintln!(
            "Detected worker blocking, signaling {} worker threads: {:?}",
            signal, targets
        );
        true
    }
}

struct StdErrThreadStateHandler;

/// ThreadStateHandler implementation that writes details to standard error.
impl ThreadStateHandler for StdErrThreadStateHandler {
    fn blocking_thread_details(
        &self,
        thread_id: libc::pthread_t,
        thread_name: Option<&str>,
        backtrace: Backtrace,
    ) {
        let name = thread_name
            .map(|n| format!(" for thread \"{}\"", n))
            .unwrap_or("".to_owned());
        eprintln!("Stack trace{}:{}\n{}", name, thread_id, backtrace);
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

extern "C" fn signal_handler(_: i32) {
    let backtrace = Backtrace::force_capture();
    get_thread_state_handler().blocking_thread_details(
        get_thread_id(),
        thread::current().name(),
        backtrace,
    );
}

static INIT_SIGNAL_HANDLER: Once = Once::new();

fn install_thread_stack_stace_handler(signal: libc::c_int) {
    INIT_SIGNAL_HANDLER.call_once(|| _install_thread_stack_stace_handler(signal))
}

fn _install_thread_stack_stace_handler(signal: libc::c_int) {
    unsafe {
        libc::signal(signal, signal_handler as libc::sighandler_t);
    }
}

fn signal_all_threads(signal: libc::c_int, targets: Vec<libc::pthread_t>) {
    for thread_id in &targets {
        let result = unsafe { libc::pthread_kill(*thread_id, signal) };
        if result != 0 {
            eprintln!("Error sending signal: {:?}", result);
        }
    }
}

/// Utility to help with "really nice to add a warning for tasks that might be blocking"
#[derive(Debug)]
pub struct LongRunningTaskDetector {
    interval: Duration,
    detection_time: Duration,
    stop_flag: Arc<Mutex<bool>>,
    workers: Arc<WorkerSet>,
    signal: libc::c_int,
    identity: usize,
}

async fn do_nothing(tx: mpsc::Sender<()>) {
    // signal I am done
    let _ = tx.send(()).unwrap();
}

fn probe(
    tokio_runtime: &Arc<Runtime>,
    detection_time: Duration,
    signal: libc::c_int,
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
        if action.blocking_detected(signal, &targets) {
            signal_all_threads(signal, targets);
            // Wait for our probe to eventually finish, we do not want to have multiple probes running at the same time.
        }
        let _ = rx.recv_timeout(get_panic_worker_block_duration()).unwrap();
    }
}

/// Utility to help with "really nice to add a warning for tasks that might be blocking"
/// Example use:
///  ```
///    use std::sync::Arc;
///    use tokio_util::lrtd::LongRunningTaskDetector;
///
///    let mut builder = tokio::runtime::Builder::new_multi_thread();
///    let mutable_builder = builder.worker_threads(2);
///    let lrtd = LongRunningTaskDetector::new(
///      std::time::Duration::from_millis(10),
///      std::time::Duration::from_millis(100),
///      libc::SIGUSR1,
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
///     Detected worker blocking, signaling SIGUSR1 worker threads: [123145318232064, 123145320341504]
///     Stack trace for thread "tokio-runtime-worker":123145318232064
///        0: std::backtrace_rs::backtrace::libunwind::trace
///                at /rustc/a28077b28a02b92985b3a3faecf92813155f1ea1/library/std/src/../../backtrace/src/backtrace/libunwind.rs:93:5
///        1: std::backtrace_rs::backtrace::trace_unsynchronized
///                at /rustc/a28077b28a02b92985b3a3faecf92813155f1ea1/library/std/src/../../backtrace/src/backtrace/mod.rs:66:5
///        2: std::backtrace::Backtrace::create
///                at /rustc/a28077b28a02b92985b3a3faecf92813155f1ea1/library/std/src/backtrace.rs:331:13
///        3: std::backtrace::Backtrace::force_capture
///                at /rustc/a28077b28a02b92985b3a3faecf92813155f1ea1/library/std/src/backtrace.rs:313:9
///        4: tokio_util::lrtd::lrtd::signal_handler
///                at ./src/lrtd/lrtd.rs:217:21
///        5: __sigtramp
///        6: ___semwait_signal
///        7: <unknown>
///        8: std::sys::unix::thread::Thread::sleep
///                at /rustc/a28077b28a02b92985b3a3faecf92813155f1ea1/library/std/src/sys/unix/thread.rs:241:20
///        9: std::thread::sleep
///                at /rustc/a28077b28a02b92985b3a3faecf92813155f1ea1/library/std/src/thread/mod.rs:872:5
///        10: lrtd::run_blocking_stuff::{{closure}}
///                at ./tests/lrtd.rs:11:5
///        11: tokio::runtime::task::core::Core<T,S>::poll::{{closure}}
///                at /Users/zoly/NetBeansProjects/tokio/tokio/src/runtime/task/core.rs:328:17
///        12: tokio::loom::std::unsafe_cell::UnsafeCell<T>::with_mut
///                at /Users/zoly/NetBeansProjects/tokio/tokio/src/loom/std/unsafe_cell.rs:16:9
///        13: tokio::runtime::task::core::Core<T,S>::poll
///  ```
///
impl LongRunningTaskDetector {
    /// Creates a new `LongRunningTaskDetector` instance.
    ///
    /// # Arguments
    ///
    /// * `interval` - The interval between probes. This interval is randomized.
    /// * `detection_time` - The maximum time allowed for a probe to succeed.
    ///                      A probe running for longer indicates something is blocking the worker threads.
    /// * `signal` - The signal to use for signaling worker threads when blocking is detected.
    /// * `runtime_builder` - A mutable reference to a `tokio::runtime::Builder`.
    ///
    /// # Returns
    ///
    /// Returns a new `LongRunningTaskDetector` instance.    
    pub fn new(
        interval: Duration,
        detection_time: Duration,
        signal: libc::c_int,
        runtime_builder: &mut Builder,
    ) -> Self {
        install_thread_stack_stace_handler(signal);
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
        let identity = get_next_global_value();
        LongRunningTaskDetector {
            interval,
            detection_time,
            stop_flag: Arc::new(Mutex::new(true)),
            workers,
            signal,
            identity,
        }
    }

    /// Starts the monitoring thread with default action handlers (that write details to std err).
    ///
    /// # Arguments
    ///
    /// * `runtime` - An `Arc` reference to a `tokio::runtime::Runtime`.    
    pub fn start(&self, runtime: Arc<Runtime>) {
        self.start_with_custom_action(
            runtime,
            Arc::new(StdErrBlockingActionHandler),
            Arc::new(StdErrThreadStateHandler),
        )
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
        thread_action: Arc<dyn ThreadStateHandler>,
    ) {
        set_thread_state_handler(thread_action, self.identity);
        *self.stop_flag.lock().unwrap() = false;
        let stop_flag = Arc::clone(&self.stop_flag);
        let detection_time = self.detection_time.clone();
        let interval = self.interval.clone();
        let signal = self.signal;
        let workers = Arc::clone(&self.workers);
        thread::spawn(move || {
            let mut rng = rand::thread_rng();
            while !*stop_flag.lock().unwrap() {
                probe(&runtime, detection_time, signal, &workers, &action);
                thread::sleep(Duration::from_micros(
                    rng.gen_range(1..=interval.as_micros()).try_into().unwrap(),
                ));
            }
        });
    }

    /// Stops the monitoring thread. Does nothing if LRTD is already stopped.
    pub fn stop(&self) {
        let mut sf = self.stop_flag.lock().unwrap();
        if *sf != true {
            *sf = true;
            reset_thread_state_handler(self.identity);
        }
    }
}

impl Drop for LongRunningTaskDetector {
    fn drop(&mut self) {
        self.stop();
    }
}
