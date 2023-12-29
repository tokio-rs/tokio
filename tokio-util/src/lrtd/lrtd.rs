use libc;
use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal};
use rand::Rng;
use std::backtrace::Backtrace;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;
use std::{env, thread};
use tokio::runtime::{Builder, Runtime};

// A static AtomicUsize variable
static mut GLOBAL_COUNTER: Option<AtomicUsize> = None;
static INIT: Once = Once::new();

// Function to initialize the global counter if not already initialized
fn initialize_global_counter() {
    INIT.call_once(|| unsafe {
        GLOBAL_COUNTER = Some(AtomicUsize::new(0));
    });
}

// Function to get the next value in the global counter
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
            panic!("Cannot overwrite action with something different!")
        }
        let mut found = false;
        for x_owner in &(self.owners) {
            if *x_owner == owner {
                panic!("Cannot set thread state handler twice by the same owner")
            }
        }
        if !found {
            self.owners.push(owner);
        }
    }

    fn remove(&mut self, owner: usize) {
        if let Some(index) = self.owners.iter().position(|&x| x == owner) {
            // Use remove() to remove the value at the found index
            let removed_value = self.owners.remove(index);
        } else {
            panic!("Cannot find owner")
        }
    }
}

static mut GLOBAL_TH_STATE_HANDLER: Mutex<Option<OwnedThreadStateHandler>> = Mutex::new(None);

fn set_thread_state_handler(action: Arc<dyn ThreadStateHandler>, owner: usize) {
    unsafe {
        if let Ok(mut value) = GLOBAL_TH_STATE_HANDLER.lock() {
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
    }
}

fn reset_thread_state_handler(owner: usize) {
    unsafe {
        if let Ok(mut value) = GLOBAL_TH_STATE_HANDLER.lock() {
            match value.as_mut() {
                Some(handler) => {
                    (*handler).remove(owner);
                }
                None => {
                    panic!("Cannot find handler");
                }
            }
        }
    }
}

fn get_thread_state_handler() -> Arc<dyn ThreadStateHandler> {
    unsafe {
        let option = GLOBAL_TH_STATE_HANDLER.lock().unwrap();
        match option.as_ref() {
            Some(value) => value.action.clone(),
            None => Arc::new(StdErrThreadStateHandler),
        }
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

pub trait ThreadStateHandler: Send + Sync {
    fn blocking_thread_details(
        &self,
        thread_id: libc::pthread_t,
        thread_name: Option<&str>,
        backtrace: Backtrace,
    );
}

pub trait BlockingActionHandler: Send + Sync {
    fn blocking_detected(&self, signal: Signal, targets: &Vec<libc::pthread_t>);
}

struct StdErrBlockingActionHandler;

impl BlockingActionHandler for StdErrBlockingActionHandler {
    fn blocking_detected(&self, signal: Signal, targets: &Vec<libc::pthread_t>) {
        eprintln!(
            "Detected worker blocking, signaling worker threads: {:?}",
            targets
        );
    }
}

struct StdErrThreadStateHandler;

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

fn install_thread_stack_stace_handler(signal: Signal) {
    INIT_SIGNAL_HANDLER.call_once(|| _install_thread_stack_stace_handler(signal))
}

fn _install_thread_stack_stace_handler(signal: Signal) {
    let mut sigset = SigSet::empty();
    sigset.add(signal);

    // Set up a signal action
    let sa = SigAction::new(
        SigHandler::Handler(signal_handler),
        SaFlags::empty(),
        sigset,
    );

    // Register the signal action for process
    unsafe {
        signal::sigaction(signal, &sa).expect("Failed to register signal handler");
    }
}

fn signal_all_threads(signal: Signal, targets: Vec<libc::pthread_t>) {
    for thread_id in &targets {
        let result = unsafe {
            libc::pthread_kill(
                *thread_id,
                match signal.into() {
                    Some(s) => s as libc::c_int,
                    None => 0,
                },
            )
        };
        if result != 0 {
            eprintln!("Error sending signal: {:?}", result);
        }
    }
}

#[derive(Debug)]
pub struct LongRunningTaskDetector {
    interval: Duration,
    detection_time: Duration,
    stop_flag: Arc<Mutex<bool>>,
    workers: Arc<WorkerSet>,
    signal: Signal,
    identity: usize,
}

async fn do_nothing(tx: mpsc::Sender<()>) {
    // signal I am done
    let _ = tx.send(()).unwrap();
}

fn probe(
    tokio_runtime: &Arc<Runtime>,
    detection_time: Duration,
    signal: Signal,
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
        action.blocking_detected(signal, &targets);

        signal_all_threads(signal, targets);
        // Wait for our probe to eventually finish, we do not want to have multiple probes running at the same time.
        let _ = rx.recv_timeout(get_panic_worker_block_duration()).unwrap();
    }
}

impl LongRunningTaskDetector {
    pub fn new(
        interval: Duration,
        detection_time: Duration,
        signal: Signal,
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
            stop_flag: Arc::new(Mutex::new(false)),
            workers,
            signal,
            identity,
        }
    }

    pub fn start(&self, runtime: Arc<Runtime>) {
        self.start_with_custom_action(
            runtime,
            Arc::new(StdErrBlockingActionHandler),
            Arc::new(StdErrThreadStateHandler),
        )
    }

    pub fn start_with_custom_action(
        &self,
        runtime: Arc<Runtime>,
        action: Arc<dyn BlockingActionHandler>,
        thread_action: Arc<dyn ThreadStateHandler>,
    ) {
        set_thread_state_handler(thread_action, self.identity);
        let stop_flag = Arc::clone(&self.stop_flag);
        let detection_time = self.detection_time.clone();
        let interval = self.interval.clone();
        let signal = self.signal.clone();
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

    pub fn stop(&self) {
        *self.stop_flag.lock().unwrap() = true;
        reset_thread_state_handler(self.identity);
    }
}
