use libc;
use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal};
use rand::Rng;
use std::backtrace::Backtrace;
use std::collections::HashSet;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{env, thread};
use tokio::runtime::{Builder, Runtime};

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
    let thread = thread::current();
    let name = thread
        .name()
        .map(|n| format!(" for thread \"{}\"", n))
        .unwrap_or("".to_owned());
    let trace = Backtrace::force_capture();
    eprintln!("Stack trace{}:{}\n{}", name, get_thread_id(), trace);
}

pub fn install_thread_stack_stace_handler(signal: Signal) {
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
) {
    let (tx, rx) = mpsc::channel();
    let _nothing_handle = tokio_runtime.spawn(do_nothing(tx));
    let is_probe_success = match rx.recv_timeout(detection_time) {
        Ok(_result) => true,
        Err(_) => false,
    };
    if !is_probe_success {
        let targets = workers.get_all();
        eprintln!(
            "Detected worker blocking, signaling worker threads: {:?}",
            targets
        );
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
            stop_flag: Arc::new(Mutex::new(false)),
            workers,
            signal,
        }
    }

    pub fn start(&self, runtime: Arc<Runtime>) {
        let stop_flag = Arc::clone(&self.stop_flag);
        let detection_time = self.detection_time.clone();
        let interval = self.interval.clone();
        let signal = self.signal.clone();
        let tokio_runtime = runtime.clone();
        let workers = Arc::clone(&self.workers);
        thread::spawn(move || {
            let mut rng = rand::thread_rng();
            while !*stop_flag.lock().unwrap() {
                probe(&tokio_runtime, detection_time, signal, &workers);
                thread::sleep(Duration::from_micros(
                    rng.gen_range(1..=interval.as_micros()).try_into().unwrap(),
                ));
            }
        });
    }

    pub fn stop(&self) {
        *self.stop_flag.lock().unwrap() = true;
    }
}
