//! `LocalEventLoop` driven the way an Emscripten host would: ticked from
//! host callbacks, with a host timer armed from `next_sleep`.

#![cfg(all(
    tokio_unstable,
    target_os = "emscripten",
    not(target_feature = "atomics"),
    feature = "rt",
    feature = "time",
    feature = "sync"
))]

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::time::{Duration, Instant};

use tokio::runtime::{Builder, LocalEventLoop};
use tokio::sync::oneshot;

extern "C" {
    /// Emscripten's `ASYNCIFY` build mode; 2 is JSPI.
    fn emscripten_has_asyncify() -> i32;
    fn emscripten_async_call(func: extern "C" fn(*mut c_void), arg: *mut c_void, millis: i32);
    /// Suspends the calling activation under JSPI.
    fn emscripten_sleep(ms: u32);
}

fn jspi_linked() -> bool {
    // SAFETY: an Emscripten libc query with no arguments and no side effects.
    unsafe { emscripten_has_asyncify() == 2 }
}

thread_local! {
    static HOSTED: RefCell<Option<LocalEventLoop>> = const { RefCell::new(None) };
}

/// The host's tick: a plain host callback, with no promising activation.
extern "C" fn host_tick(_: *mut c_void) {
    HOSTED.with(|hosted| {
        let mut hosted = hosted.borrow_mut();
        let Some(el) = hosted.as_mut() else { return };
        loop {
            el.tick(64);
            match el.next_sleep() {
                Some(Duration::ZERO) => {}
                Some(timeout) => return post_tick(timeout),
                None => return,
            }
        }
    })
}

fn post_tick(after: Duration) {
    // SAFETY: schedules `host_tick` on the host loop; it takes no argument.
    unsafe { emscripten_async_call(host_tick, std::ptr::null_mut(), after.as_millis() as i32) }
}

/// Hands the loop to the host and waits, suspended without any runtime
/// entered, until a timer task on it has run.
fn run_hosted(builder: &mut Builder) {
    let el = builder
        .on_work_available(|| post_tick(Duration::ZERO))
        .build_local_event_loop(Default::default())
        .unwrap();
    let done = Rc::new(Cell::new(false));
    let d = done.clone();
    el.spawn_local(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        d.set(true);
    });
    HOSTED.with(|hosted| *hosted.borrow_mut() = Some(el));
    post_tick(Duration::ZERO);

    let start = Instant::now();
    while !done.get() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "the hosted loop stalled"
        );
        // SAFETY: an Emscripten API; under JSPI it suspends this activation.
        unsafe { emscripten_sleep(5) };
    }
    HOSTED.with(|hosted| hosted.borrow_mut().take());
}

// A tick ends with a zero-duration turn of the time driver's park, which
// must not suspend: a host callback has nothing to suspend.
#[test]
fn tick_from_host_callback() {
    if !jspi_linked() {
        return;
    }
    run_hosted(Builder::new_current_thread().enable_time());
}

// The same through the I/O driver's zero-timeout `epoll_wait`.
#[cfg(feature = "net")]
#[test]
fn tick_from_host_callback_with_io_driver() {
    if !jspi_linked() {
        return;
    }
    run_hosted(Builder::new_current_thread().enable_all());
}

// The reactor is an epoll set, whose descriptor the host listens on.
#[cfg(feature = "net")]
#[test]
fn io_driver_exposes_event_fd() {
    let el = Builder::new_current_thread()
        .enable_all()
        .build_local_event_loop(Default::default())
        .unwrap();
    assert!(el.event_fd().is_some());
}

#[test]
fn timers_fire_when_driven_by_next_sleep() {
    let mut el = Builder::new_current_thread()
        .enable_time()
        .build_local_event_loop(Default::default())
        .unwrap();
    let done = Rc::new(Cell::new(false));
    let d = done.clone();
    el.spawn_local(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        d.set(true);
    });

    let start = Instant::now();
    loop {
        el.tick(64);
        if done.get() {
            break;
        }
        match el.next_sleep() {
            Some(Duration::ZERO) => {}
            Some(timeout) => std::thread::sleep(timeout),
            None => panic!("a pending timer bounds next_sleep"),
        }
    }
    assert!(start.elapsed() >= Duration::from_millis(10));
}

#[test]
fn wake_while_idle_notifies_the_host() {
    static NOTIFIED: AtomicUsize = AtomicUsize::new(0);
    let mut el = Builder::new_current_thread()
        .enable_time()
        .on_work_available(|| {
            NOTIFIED.fetch_add(1, SeqCst);
        })
        .build_local_event_loop(Default::default())
        .unwrap();
    let (tx, rx) = oneshot::channel::<u32>();
    let got = Rc::new(Cell::new(None));
    let g = got.clone();
    el.spawn_local(async move { g.set(Some(rx.await.unwrap())) });

    el.tick(64);
    assert_eq!(el.next_sleep(), None);
    let before = NOTIFIED.load(SeqCst);
    tx.send(7).unwrap();
    assert_eq!(NOTIFIED.load(SeqCst), before + 1);

    el.tick(64);
    assert_eq!(got.get(), Some(7));
}
