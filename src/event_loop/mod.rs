use std::cell::RefCell;
use std::io::{self, ErrorKind};
use std::marker;
use std::mem;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};
use std::time::{Instant, Duration};

use futures::{Future, Poll};
use futures::task::{self, Task, Notify, TaskHandle};
use futures::executor::{ExecuteCallback, Executor};
use mio;
use slab::Slab;

use slot::{self, Slot};
use timer_wheel::{TimerWheel, Timeout};

mod channel;
mod loop_data;
mod source;
mod timeout;
pub use self::loop_data::{LoopData, AddLoopData};
pub use self::source::{AddSource, IoToken};
pub use self::timeout::{AddTimeout, TimeoutToken};
use self::channel::{Sender, Receiver, channel};

static NEXT_LOOP_ID: AtomicUsize = ATOMIC_USIZE_INIT;
scoped_thread_local!(static CURRENT_LOOP: Loop);

const SLAB_CAPACITY: usize = 1024 * 64;

/// An event loop.
///
/// The event loop is the main source of blocking in an application which drives
/// all other I/O events and notifications happening. Each event loop can have
/// multiple handles pointing to it, each of which can then be used to create
/// various I/O objects to interact with the event loop in interesting ways.
// TODO: expand this
pub struct Loop {
    id: usize,
    io: mio::Poll,
    events: mio::Events,
    tx: Arc<MioSender>,
    rx: Receiver<Message>,
    dispatch: RefCell<Slab<Scheduled, usize>>,
    _future_registration: mio::Registration,
    future_readiness: Arc<mio::SetReadiness>,

    // Timer wheel keeping track of all timeouts. The `usize` stored in the
    // timer wheel is an index into the slab below.
    //
    // The slab below keeps track of the timeouts themselves as well as the
    // state of the timeout itself. The `TimeoutToken` type is an index into the
    // `timeouts` slab.
    timer_wheel: RefCell<TimerWheel<usize>>,
    timeouts: RefCell<Slab<(Timeout, TimeoutState), usize>>,

    // A `Loop` cannot be sent to other threads as it's used as a proxy for data
    // that belongs to the thread the loop was running on at some point. In
    // other words, the safety of `DropBox` below relies on loops not crossing
    // threads.
    _marker: marker::PhantomData<Rc<u32>>,
}

struct MioSender {
    inner: Sender<Message>,
}

/// Handle to an event loop, used to construct I/O objects, send messages, and
/// otherwise interact indirectly with the event loop itself.
///
/// Handles can be cloned, and when cloned they will still refer to the
/// same underlying event loop.
#[derive(Clone)]
pub struct LoopHandle {
    id: usize,
    tx: Arc<MioSender>,
}

/// A non-sendable handle to an event loop, useful for manufacturing instances
/// of `LoopData`.
#[derive(Clone)]
pub struct LoopPin {
    handle: LoopHandle,
    _marker: marker::PhantomData<Box<Drop>>,
}

struct Scheduled {
    readiness: Arc<AtomicUsize>,
    reader: Option<TaskHandle>,
    writer: Option<TaskHandle>,
}

enum TimeoutState {
    NotFired,
    Fired,
    Waiting(TaskHandle),
}

enum Direction {
    Read,
    Write,
}

enum Message {
    DropSource(usize),
    Schedule(usize, TaskHandle, Direction),
    AddTimeout(Instant, Arc<Slot<io::Result<(usize, Instant)>>>),
    UpdateTimeout(usize, TaskHandle),
    CancelTimeout(usize),
    Run(Box<ExecuteCallback>),
    Drop(loop_data::Opaque),
}

impl Loop {
    /// Creates a new event loop, returning any error that happened during the
    /// creation.
    pub fn new() -> io::Result<Loop> {
        let (tx, rx) = channel();
        let io = try!(mio::Poll::new());
        try!(io.register(&rx,
                         mio::Token(0),
                         mio::Ready::readable(),
                         mio::PollOpt::edge()));
        let pair = mio::Registration::new(&io,
                                          mio::Token(1),
                                          mio::Ready::readable(),
                                          mio::PollOpt::level());
        let (registration, readiness) = pair;
        Ok(Loop {
            id: NEXT_LOOP_ID.fetch_add(1, Ordering::Relaxed),
            io: io,
            events: mio::Events::new(),
            tx: Arc::new(MioSender { inner: tx }),
            rx: rx,
            _future_registration: registration,
            future_readiness: Arc::new(readiness),
            dispatch: RefCell::new(Slab::new_starting_at(2, SLAB_CAPACITY)),
            timeouts: RefCell::new(Slab::new_starting_at(0, SLAB_CAPACITY)),
            timer_wheel: RefCell::new(TimerWheel::new()),
            _marker: marker::PhantomData,
        })
    }

    /// Generates a handle to this event loop used to construct I/O objects and
    /// send messages.
    ///
    /// Handles to an event loop are cloneable as well and clones will always
    /// refer to the same event loop.
    pub fn handle(&self) -> LoopHandle {
        LoopHandle {
            id: self.id,
            tx: self.tx.clone(),
        }
    }

    /// Returns a "pin" of this event loop which cannot be sent across threads
    /// but can be used as a proxy to the event loop itself.
    ///
    /// Currently the primary use for this is to use as a handle to add data
    /// to the event loop directly. The `LoopPin::add_loop_data` method can
    /// be used to immediately create instances of `LoopData` structures.
    pub fn pin(&self) -> LoopPin {
        LoopPin {
            handle: self.handle(),
            _marker: marker::PhantomData,
        }
    }

    /// Runs a future until completion, driving the event loop while we're
    /// otherwise waiting for the future to complete.
    ///
    /// This function will begin executing the event loop and will finish once
    /// the provided future is resolve. Note that the future argument here
    /// crucially does not require the `'static` nor `Send` bounds. As a result
    /// the future will be "pinned" to not only this thread but also this stack
    /// frame.
    ///
    /// This function will returns the value that the future resolves to once
    /// the future has finished. If the future never resolves then this function
    /// will never return.
    ///
    /// # Panics
    ///
    /// This method will **not** catch panics from polling the future `f`. If
    /// the future panics then it's the responsibility of the caller to catch
    /// that panic and handle it as appropriate.
    ///
    /// Similarly, becuase the provided future will be pinned not only to this
    /// thread but also to this task, any attempt to poll the future on a
    /// separate thread will result in a panic. That is, calls to
    /// `task::poll_on` must be avoided.
    pub fn run<F>(&mut self, mut f: F) -> Result<F::Item, F::Error>
        where F: Future,
    {
        struct MyNotify(Arc<mio::SetReadiness>);

        impl Notify for MyNotify {
            fn notify(&self) {
                self.0.set_readiness(mio::Ready::readable())
                      .expect("failed to set readiness");
            }
        }

        // First up, create the task that will drive this future. The task here
        // isn't a "normal task" but rather one where we define what to do when
        // a readiness notification comes in.
        //
        // We translate readiness notifications to a `set_readiness` of our
        // `future_readiness` structure we have stored internally.
        let mut task = Task::new_notify(MyNotify(self.future_readiness.clone()));
        let ready = self.future_readiness.clone();

        // Next, move all that data into a dynamically dispatched closure to cut
        // down on monomorphization costs. Inside this closure we unset the
        // readiness of the future (as we're about to poll it) and then we check
        // to see if it's done. If it's not then the event loop will turn again.
        let mut res = None;
        self._run(&mut || {
            ready.set_readiness(mio::Ready::none())
                 .expect("failed to set readiness");
            assert!(res.is_none());
            match task.enter(|| f.poll()) {
                Poll::NotReady => {}
                Poll::Ok(e) => res = Some(Ok(e)),
                Poll::Err(e) => res = Some(Err(e)),
            }
            res.is_some()
        });
        res.expect("run should not return until future is done")
    }

    fn _run(&mut self, done: &mut FnMut() -> bool) {
        // Check to see if we're done immediately, if so we shouldn't do any
        // work.
        if CURRENT_LOOP.set(self, || done()) {
            return
        }

        loop {
            let amt;
            // On Linux, Poll::poll is epoll_wait, which may return EINTR if a
            // ptracer attaches. This retry loop prevents crashing when
            // attaching strace, or similar.
            let start = Instant::now();
            loop {
                let timeout = self.timer_wheel.borrow().next_timeout().map(|t| {
                    if t < start {
                        Duration::new(0, 0)
                    } else {
                        t - start
                    }
                });
                match self.io.poll(&mut self.events, timeout) {
                    Ok(a) => {
                        amt = a;
                        break;
                    }
                    Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
                    err @ Err(_) => {
                        err.unwrap();
                    }
                }
            }
            debug!("loop poll - {:?}", start.elapsed());
            debug!("loop time - {:?}", Instant::now());

            // First up, process all timeouts that may have just occurred.
            let start = Instant::now();
            self.consume_timeouts(start);

            // Next, process all the events that came in.
            for i in 0..self.events.len() {
                let event = self.events.get(i).unwrap();
                let token = usize::from(event.token());

                // Token 0 == our incoming message queue, so this means we
                // process the whole queue of messages.
                //
                // Token 1 == we should poll the future, we'll do that right
                // after we get through the rest of this tick of the event loop.
                if token == 0 {
                    debug!("consuming notification queue");
                    CURRENT_LOOP.set(&self, || {
                        self.consume_queue();
                    });
                    continue
                } else if token == 1 {
                    if CURRENT_LOOP.set(self, || done()) {
                        return
                    }
                    continue
                }

                trace!("event {:?} {:?}", event.kind(), event.token());

                // For any other token we look at `dispatch` to see what we're
                // supposed to do. If there's a waiter we get ready to notify
                // it, and we also or-in atomically any events that have
                // happened (currently read/write events).
                let mut reader = None;
                let mut writer = None;
                if let Some(sched) = self.dispatch.borrow_mut().get_mut(token) {
                    if event.kind().is_readable() {
                        reader = sched.reader.take();
                        sched.readiness.fetch_or(1, Ordering::Relaxed);
                    }
                    if event.kind().is_writable() {
                        writer = sched.writer.take();
                        sched.readiness.fetch_or(2, Ordering::Relaxed);
                    }
                } else {
                    debug!("notified on {} which no longer exists", token);
                }

                // If we actually got a waiter, then notify!
                //
                // TODO: don't notify the same task twice
                if let Some(reader) = reader {
                    self.notify_handle(reader);
                }
                if let Some(writer) = writer {
                    self.notify_handle(writer);
                }
            }

            debug!("loop process - {} events, {:?}", amt, start.elapsed());
        }
    }

    fn consume_timeouts(&mut self, now: Instant) {
        loop {
            let idx = match self.timer_wheel.borrow_mut().poll(now) {
                Some(idx) => idx,
                None => break,
            };
            trace!("firing timeout: {}", idx);
            let handle = self.timeouts.borrow_mut()[idx].1.fire();
            if let Some(handle) = handle {
                self.notify_handle(handle);
            }
        }
    }

    /// Method used to notify a task handle.
    ///
    /// Note that this should be used instead fo `handle.unpark()` to ensure
    /// that the `CURRENT_LOOP` variable is set appropriately.
    fn notify_handle(&self, handle: TaskHandle) {
        debug!("notifying a task handle");
        CURRENT_LOOP.set(&self, || handle.unpark());
    }

    fn add_source(&self, source: &mio::Evented)
                  -> io::Result<(Arc<AtomicUsize>, usize)> {
        debug!("adding a new I/O source");
        let sched = Scheduled {
            readiness: Arc::new(AtomicUsize::new(0)),
            reader: None,
            writer: None,
        };
        let mut dispatch = self.dispatch.borrow_mut();
        if dispatch.vacant_entry().is_none() {
            let amt = dispatch.count();
            dispatch.grow(amt);
        }
        let entry = dispatch.vacant_entry().unwrap();
        try!(self.io.register(source,
                              mio::Token(entry.index()),
                              mio::Ready::readable() | mio::Ready::writable(),
                              mio::PollOpt::edge()));
        Ok((sched.readiness.clone(), entry.insert(sched).index()))
    }

    fn drop_source(&self, token: usize) {
        debug!("dropping I/O source: {}", token);
        self.dispatch.borrow_mut().remove(token).unwrap();
    }

    fn schedule(&self, token: usize, wake: TaskHandle, dir: Direction) {
        debug!("scheduling direction for: {}", token);
        let to_call = {
            let mut dispatch = self.dispatch.borrow_mut();
            let sched = dispatch.get_mut(token).unwrap();
            let (slot, bit) = match dir {
                Direction::Read => (&mut sched.reader, 1),
                Direction::Write => (&mut sched.writer, 2),
            };
            let ready = sched.readiness.load(Ordering::SeqCst);
            if ready & bit != 0 {
                *slot = None;
                sched.readiness.store(ready & !bit, Ordering::SeqCst);
                Some(wake)
            } else {
                *slot = Some(wake);
                None
            }
        };
        if let Some(to_call) = to_call {
            debug!("schedule immediately done");
            self.notify_handle(to_call);
        }
    }

    fn add_timeout(&self, at: Instant) -> io::Result<(usize, Instant)> {
        let mut timeouts = self.timeouts.borrow_mut();
        if timeouts.vacant_entry().is_none() {
            let len = timeouts.count();
            timeouts.grow(len);
        }
        let entry = timeouts.vacant_entry().unwrap();
        let timeout = self.timer_wheel.borrow_mut().insert(at, entry.index());
        let when = *timeout.when();
        let entry = entry.insert((timeout, TimeoutState::NotFired));
        debug!("added a timeout: {}", entry.index());
        Ok((entry.index(), when))
    }

    fn update_timeout(&self, token: usize, handle: TaskHandle) {
        debug!("updating a timeout: {}", token);
        let to_wake = self.timeouts.borrow_mut()[token].1.block(handle);
        if let Some(to_wake) = to_wake {
            self.notify_handle(to_wake);
        }
    }

    fn cancel_timeout(&self, token: usize) {
        debug!("cancel a timeout: {}", token);
        let pair = self.timeouts.borrow_mut().remove(token);
        if let Some((timeout, _state)) = pair {
            self.timer_wheel.borrow_mut().cancel(&timeout);
        }
    }

    fn consume_queue(&self) {
        // TODO: can we do better than `.unwrap()` here?
        while let Some(msg) = self.rx.recv().unwrap() {
            self.notify(msg);
        }
    }

    fn notify(&self, msg: Message) {
        match msg {
            Message::DropSource(tok) => self.drop_source(tok),
            Message::Schedule(tok, wake, dir) => self.schedule(tok, wake, dir),

            Message::AddTimeout(at, slot) => {
                slot.try_produce(self.add_timeout(at))
                    .ok().expect("interference with try_produce on timeout");
            }
            Message::UpdateTimeout(t, handle) => self.update_timeout(t, handle),
            Message::CancelTimeout(t) => self.cancel_timeout(t),
            Message::Run(f) => {
                debug!("running a closure");
                f.call()
            }
            Message::Drop(data) => {
                debug!("dropping some data");
                drop(data);
            }
        }
    }
}

impl LoopHandle {
    fn send(&self, msg: Message) {
        self.with_loop(|lp| {
            match lp {
                Some(lp) => {
                    // Need to execute all existing requests first, to ensure
                    // that our message is processed "in order"
                    lp.consume_queue();
                    lp.notify(msg);
                }
                None => {
                    match self.tx.inner.send(msg) {
                        Ok(()) => {}

                        // This should only happen when there was an error
                        // writing to the pipe to wake up the event loop,
                        // hopefully that never happens
                        Err(e) => {
                            panic!("error sending message to event loop: {}", e)
                        }
                    }
                }
            }
        })
    }

    fn with_loop<F, R>(&self, f: F) -> R
        where F: FnOnce(Option<&Loop>) -> R
    {
        if CURRENT_LOOP.is_set() {
            CURRENT_LOOP.with(|lp| {
                if lp.id == self.id {
                    f(Some(lp))
                } else {
                    f(None)
                }
            })
        } else {
            f(None)
        }
    }
}

impl LoopPin {
    /// Returns a reference to the underlying handle to the event loop.
    pub fn handle(&self) -> &LoopHandle {
        &self.handle
    }

    /// TODO: dox
    pub fn executor(&self) -> Arc<Executor> {
        self.handle.tx.clone()
    }
}

struct LoopFuture<T, U> {
    loop_handle: LoopHandle,
    data: Option<U>,
    result: Option<(Arc<Slot<io::Result<T>>>, slot::Token)>,
}

impl<T, U> LoopFuture<T, U>
    where T: 'static,
{
    fn poll<F, G>(&mut self, f: F, g: G) -> Poll<T, io::Error>
        where F: FnOnce(&Loop, U) -> io::Result<T>,
              G: FnOnce(U, Arc<Slot<io::Result<T>>>) -> Message,
    {
        match self.result {
            Some((ref result, ref mut token)) => {
                result.cancel(*token);
                match result.try_consume() {
                    Ok(t) => return t.into(),
                    Err(_) => {}
                }
                let task = task::park();
                *token = result.on_full(move |_| {
                    task.unpark();
                });
                return Poll::NotReady
            }
            None => {
                let data = &mut self.data;
                let ret = self.loop_handle.with_loop(|lp| {
                    lp.map(|lp| f(lp, data.take().unwrap()))
                });
                if let Some(ret) = ret {
                    debug!("loop future done immediately on event loop");
                    return ret.into()
                }
                debug!("loop future needs to send info to event loop");

                let task = task::park();
                let result = Arc::new(Slot::new(None));
                let token = result.on_full(move |_| {
                    task.unpark();
                });
                self.result = Some((result.clone(), token));
                self.loop_handle.send(g(data.take().unwrap(), result));
                Poll::NotReady
            }
        }
    }
}

impl TimeoutState {
    fn block(&mut self, handle: TaskHandle) -> Option<TaskHandle> {
        match *self {
            TimeoutState::Fired => return Some(handle),
            _ => {}
        }
        *self = TimeoutState::Waiting(handle);
        None
    }

    fn fire(&mut self) -> Option<TaskHandle> {
        match mem::replace(self, TimeoutState::Fired) {
            TimeoutState::NotFired => None,
            TimeoutState::Fired => panic!("fired twice?"),
            TimeoutState::Waiting(handle) => Some(handle),
        }
    }
}

impl Executor for MioSender {
    fn execute_boxed(&self, callback: Box<ExecuteCallback>) {
        self.inner.send(Message::Run(callback))
            .expect("error sending a message to the event loop")
    }
}
