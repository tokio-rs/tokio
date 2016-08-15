use std::cell::{Cell, RefCell};
use std::io::{self, ErrorKind};
use std::marker;
use std::mem;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};
use std::sync::mpsc;
use std::time::{Instant, Duration};

use futures::{Future, Poll};
use futures::task::{self, TaskHandle};
use futures::executor::{ExecuteCallback, Executor};
use mio;
use slab::Slab;

use channel::{Sender, Receiver, channel};
use event_loop::dropbox::DropBox;
use slot::{self, Slot};
use timer_wheel::{TimerWheel, Timeout};

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
    active: Cell<bool>,
    io: mio::Poll,
    tx: Arc<MioSender>,
    rx: Receiver<Message>,
    dispatch: RefCell<Slab<Scheduled, usize>>,

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
    source: IoSource,
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
    AddSource(IoSource, Arc<Slot<io::Result<usize>>>),
    DropSource(usize),
    Schedule(usize, TaskHandle, Direction),
    AddTimeout(Instant, Arc<Slot<io::Result<TimeoutToken>>>),
    UpdateTimeout(TimeoutToken, TaskHandle),
    CancelTimeout(TimeoutToken),
    Run(Box<ExecuteCallback>),
    Drop(DropBox<dropbox::MyDrop>),
    Shutdown,
}

/// Type of I/O objects inserted into the event loop, created by `Source::new`.
pub struct Source<E: ?Sized> {
    readiness: AtomicUsize,
    io: E,
}

/// I/O objects inserted into the event loop
pub type IoSource = Arc<Source<mio::Evented + Sync + Send>>;

fn register(poll: &mio::Poll,
            token: usize,
            sched: &Scheduled) -> io::Result<()> {
    poll.register(&sched.source.io,
                  mio::Token(token),
                  mio::EventSet::readable() | mio::EventSet::writable(),
                  mio::PollOpt::edge())
}

fn deregister(poll: &mio::Poll, sched: &Scheduled) {
    // TODO: handle error
    poll.deregister(&sched.source.io).unwrap();
}

impl Loop {
    /// Creates a new event loop, returning any error that happened during the
    /// creation.
    pub fn new() -> io::Result<Loop> {
        let (tx, rx) = channel();
        let io = try!(mio::Poll::new());
        try!(io.register(&rx,
                         mio::Token(0),
                         mio::EventSet::readable(),
                         mio::PollOpt::edge()));
        Ok(Loop {
            id: NEXT_LOOP_ID.fetch_add(1, Ordering::Relaxed),
            active: Cell::new(true),
            io: io,
            tx: Arc::new(MioSender { inner: tx }),
            rx: rx,
            dispatch: RefCell::new(Slab::new_starting_at(1, SLAB_CAPACITY)),
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

    /// Creates a new `LoopData<A>` handle by associating data to be directly
    /// stored by this event loop.
    ///
    /// This function is useful for when storing non-`Send` data inside of a
    /// future. The `LoopData<A>` handle is itself `Send + 'static` regardless
    /// of the underlying `A`. That is, for example, you can create a handle to
    /// some data that contains an `Rc`, for example.
    pub fn add_loop_data<A>(&self, a: A) -> LoopData<A>
        where A: 'static,
    {
        self.pin().add_loop_data(a)
    }

    /// Runs a future until completion, driving the event loop while we're
    /// otherwise waiting for the future to complete.
    ///
    /// Returns the value that the future resolves to.
    pub fn run<F>(&mut self, f: F) -> Result<F::Item, F::Error>
        where F: Future + 'static,
    {
        let (tx_res, rx_res) = mpsc::channel();
        let handle = self.handle();
        self.add_loop_data(f.then(move |res| {
            handle.shutdown();
            tx_res.send(res)
        })).forget();

        self._run();

        rx_res.recv().unwrap()
    }

    fn _run(&mut self) {
        let mut events = mio::Events::new();
        self.active.set(true);
        while self.active.get() {
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
                match self.io.poll(&mut events, timeout) {
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
            for i in 0..events.len() {
                let event = events.get(i).unwrap();
                let token = usize::from(event.token());

                // Token 0 == our incoming message queue, so this means we
                // process the whole queue of messages.
                if token == 0 {
                    debug!("consuming notification queue");
                    CURRENT_LOOP.set(&self, || {
                        self.consume_queue();
                    });
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
                        sched.source.readiness.fetch_or(1, Ordering::Relaxed);
                    }
                    if event.kind().is_writable() {
                        writer = sched.writer.take();
                        sched.source.readiness.fetch_or(2, Ordering::Relaxed);
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

        debug!("loop is done!");
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

    fn add_source(&self, source: IoSource) -> io::Result<usize> {
        debug!("adding a new I/O source");
        let sched = Scheduled {
            source: source,
            reader: None,
            writer: None,
        };
        let mut dispatch = self.dispatch.borrow_mut();
        if dispatch.vacant_entry().is_none() {
            let amt = dispatch.count();
            dispatch.grow(amt);
        }
        let entry = dispatch.vacant_entry().unwrap();
        try!(register(&self.io, entry.index(), &sched));
        Ok(entry.insert(sched).index())
    }

    fn drop_source(&self, token: usize) {
        debug!("dropping I/O source: {}", token);
        let sched = self.dispatch.borrow_mut().remove(token).unwrap();
        deregister(&self.io, &sched);
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
            let ready = sched.source.readiness.load(Ordering::SeqCst);
            if ready & bit != 0 {
                *slot = None;
                sched.source.readiness.store(ready & !bit, Ordering::SeqCst);
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

    fn add_timeout(&self, at: Instant) -> io::Result<TimeoutToken> {
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
        Ok(TimeoutToken {
            token: entry.index(),
            when: when,
        })
    }

    fn update_timeout(&self, token: &TimeoutToken, handle: TaskHandle) {
        debug!("updating a timeout: {}", token.token);
        let to_wake = self.timeouts.borrow_mut()[token.token].1.block(handle);
        if let Some(to_wake) = to_wake {
            self.notify_handle(to_wake);
        }
    }

    fn cancel_timeout(&self, token: &TimeoutToken) {
        debug!("cancel a timeout: {}", token.token);
        let pair = self.timeouts.borrow_mut().remove(token.token);
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
            Message::AddSource(source, slot) => {
                // This unwrap() should always be ok as we're the only producer
                slot.try_produce(self.add_source(source))
                    .ok().expect("interference with try_produce");
            }
            Message::DropSource(tok) => self.drop_source(tok),
            Message::Schedule(tok, wake, dir) => self.schedule(tok, wake, dir),
            Message::Shutdown => self.active.set(false),

            Message::AddTimeout(at, slot) => {
                slot.try_produce(self.add_timeout(at))
                    .ok().expect("interference with try_produce on timeout");
            }
            Message::UpdateTimeout(t, handle) => self.update_timeout(&t, handle),
            Message::CancelTimeout(t) => self.cancel_timeout(&t),
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

    /// Add a new source to an event loop, returning a future which will resolve
    /// to the token that can be used to identify this source.
    ///
    /// When a new I/O object is created it needs to be communicated to the
    /// event loop to ensure that it's registered and ready to receive
    /// notifications. The event loop with then respond with a unique token that
    /// this handle can be identified with (the resolved value of the returned
    /// future).
    ///
    /// This token is then passed in turn to each of the methods below to
    /// interact with notifications on the I/O object itself.
    ///
    /// # Panics
    ///
    /// The returned future will panic if the event loop this handle is
    /// associated with has gone away, or if there is an error communicating
    /// with the event loop.
    pub fn add_source(&self, source: IoSource) -> AddSource {
        AddSource {
            inner: LoopFuture {
                loop_handle: self.clone(),
                data: Some(source),
                result: None,
            }
        }
    }

    /// Begin listening for read events on an event loop.
    ///
    /// Once an I/O object has been registered with the event loop through the
    /// `add_source` method, this method can be used with the assigned token to
    /// begin awaiting read notifications.
    ///
    /// Currently the current task will be notified with *edge* semantics. This
    /// means that whenever the underlying I/O object changes state, e.g. it was
    /// not readable and now it is, then a notification will be sent.
    ///
    /// # Panics
    ///
    /// This function will panic if the event loop this handle is associated
    /// with has gone away, or if there is an error communicating with the event
    /// loop.
    ///
    /// This function will also panic if there is not a currently running future
    /// task.
    pub fn schedule_read(&self, tok: usize) {
        self.send(Message::Schedule(tok, task::park(), Direction::Read));
    }

    /// Begin listening for write events on an event loop.
    ///
    /// Once an I/O object has been registered with the event loop through the
    /// `add_source` method, this method can be used with the assigned token to
    /// begin awaiting write notifications.
    ///
    /// Currently the current task will be notified with *edge* semantics. This
    /// means that whenever the underlying I/O object changes state, e.g. it was
    /// not writable and now it is, then a notification will be sent.
    ///
    /// # Panics
    ///
    /// This function will panic if the event loop this handle is associated
    /// with has gone away, or if there is an error communicating with the event
    /// loop.
    ///
    /// This function will also panic if there is not a currently running future
    /// task.
    pub fn schedule_write(&self, tok: usize) {
        self.send(Message::Schedule(tok, task::park(), Direction::Write));
    }

    /// Unregister all information associated with a token on an event loop,
    /// deallocating all internal resources assigned to the given token.
    ///
    /// This method should be called whenever a source of events is being
    /// destroyed. This will ensure that the event loop can reuse `tok` for
    /// another I/O object if necessary and also remove it from any poll
    /// notifications and callbacks.
    ///
    /// Note that wake callbacks may still be invoked after this method is
    /// called as it may take some time for the message to drop a source to
    /// reach the event loop. Despite this fact, this method will attempt to
    /// ensure that the callbacks are **not** invoked, so pending scheduled
    /// callbacks cannot be relied upon to get called.
    ///
    /// # Panics
    ///
    /// This function will panic if the event loop this handle is associated
    /// with has gone away, or if there is an error communicating with the event
    /// loop.
    pub fn drop_source(&self, tok: usize) {
        self.send(Message::DropSource(tok));
    }

    /// Adds a new timeout to get fired at the specified instant, notifying the
    /// specified task.
    pub fn add_timeout(&self, at: Instant) -> AddTimeout {
        AddTimeout {
            inner: LoopFuture {
                loop_handle: self.clone(),
                data: Some(at),
                result: None,
            },
        }
    }

    /// Updates a previously added timeout to notify a new task instead.
    ///
    /// # Panics
    ///
    /// This method will panic if the timeout specified was not created by this
    /// loop handle's `add_timeout` method.
    pub fn update_timeout(&self, timeout: &TimeoutToken) {
        let timeout = TimeoutToken { token: timeout.token, when: timeout.when };
        self.send(Message::UpdateTimeout(timeout, task::park()))
    }

    /// Cancel a previously added timeout.
    ///
    /// # Panics
    ///
    /// This method will panic if the timeout specified was not created by this
    /// loop handle's `add_timeout` method.
    pub fn cancel_timeout(&self, timeout: &TimeoutToken) {
        let timeout = TimeoutToken { token: timeout.token, when: timeout.when };
        self.send(Message::CancelTimeout(timeout))
    }

    /// Schedules a closure to add some data to event loop thread itself.
    ///
    /// This function is useful for when storing non-`Send` data inside of a
    /// future. This returns a future which will resolve to a `LoopData<A>`
    /// handle, which is itself `Send + 'static` regardless of the underlying
    /// `A`. That is, for example, you can create a handle to some data that
    /// contains an `Rc`, for example.
    ///
    /// This function takes a closure which may be sent to the event loop to
    /// generate an instance of type `A`. The closure itself is required to be
    /// `Send + 'static`, but the data it produces is only required to adhere to
    /// `'static`.
    ///
    /// If the returned future is polled on the event loop thread itself it will
    /// very cheaply resolve to a handle to the data, but if it's not polled on
    /// the event loop then it will send a message to the event loop to run the
    /// closure `f`, generate a handle, and then the future will yield it back.
    // TODO: more with examples
    pub fn add_loop_data<F, A>(&self, f: F) -> AddLoopData<F, A>
        where F: FnOnce() -> A + Send + 'static,
              A: 'static,
    {
        AddLoopData {
            inner: LoopFuture {
                loop_handle: self.clone(),
                data: Some(f),
                result: None,
            },
        }
    }

    /// Send a message to the associated event loop that it should shut down, or
    /// otherwise break out of its current loop of iteration.
    ///
    /// This method does not forcibly cause the event loop to shut down or
    /// perform an interrupt on whatever task is currently running, instead a
    /// message is simply enqueued to at a later date process the request to
    /// stop looping ASAP.
    ///
    /// # Panics
    ///
    /// This function will panic if the event loop this handle is associated
    /// with has gone away, or if there is an error communicating with the event
    /// loop.
    pub fn shutdown(&self) {
        self.send(Message::Shutdown);
    }
}

impl LoopPin {
    /// Adds some data to the event loop this pin is associated with.
    ///
    /// This method will return a handle to the data, `LoopData`, which can be
    /// used to access the underlying data whenever it's on the correct event
    /// loop thread.
    pub fn add_loop_data<A>(&self, a: A) -> LoopData<A>
        where A: 'static,
    {
        LoopData {
            data: DropBox::new_on(a, self),
            handle: self.handle.clone(),
        }
    }

    /// Returns a reference to the underlying handle to the event loop.
    pub fn handle(&self) -> &LoopHandle {
        &self.handle
    }

    /// TODO: dox
    pub fn executor(&self) -> Arc<Executor> {
        self.handle.tx.clone()
    }
}

/// A future which will resolve a unique `tok` token for an I/O object.
///
/// Created through the `LoopHandle::add_source` method, this future can also
/// resolve to an error if there's an issue communicating with the event loop.
pub struct AddSource {
    inner: LoopFuture<usize, IoSource>,
}

impl Future for AddSource {
    type Item = usize;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<usize, io::Error> {
        self.inner.poll(Loop::add_source, Message::AddSource)
    }
}

/// Return value from the `LoopHandle::add_timeout` method, a future that will
/// resolve to a `TimeoutToken` to configure the behavior of that timeout.
pub struct AddTimeout {
    inner: LoopFuture<TimeoutToken, Instant>,
}

/// A token that identifies an active timeout.
pub struct TimeoutToken {
    token: usize,
    when: Instant,
}

impl Future for AddTimeout {
    type Item = TimeoutToken;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<TimeoutToken, io::Error> {
        self.inner.poll(Loop::add_timeout, Message::AddTimeout)
    }
}

impl TimeoutToken {
    /// Returns the instant in time when this timeout token will "fire".
    ///
    /// Note that this instant may *not* be the instant that was passed in when
    /// the timeout was created. The event loop does not support high resolution
    /// timers, so the exact resolution of when a timeout may fire may be
    /// slightly fudged.
    pub fn when(&self) -> &Instant {
        &self.when
    }
}

/// A handle to data that is owned by an event loop thread, and is only
/// accessible on that thread itself.
///
/// This structure is created by the `LoopHandle::add_loop_data` method which
/// will return a future resolving to one of these references. A `LoopData<A>`
/// handle is `Send` regardless of what `A` is, but the internal data can only
/// be accessed on the event loop thread itself.
///
/// Internally this reference also stores a handle to the event loop that the
/// data originated on, so it knows how to go back to the event loop to access
/// the data itself.
// TODO: write more once it's implemented
pub struct LoopData<A: 'static> {
    data: DropBox<A>,
    handle: LoopHandle,
}

/// Future returned from the `LoopHandle::add_loop_data` method.
///
/// This future will resolve to a `LoopData<A>` reference when completed, which
/// represents a handle to data that is "owned" by the event loop thread but can
/// migrate among threads temporarily so travel with a future itself.
pub struct AddLoopData<F, A> {
    inner: LoopFuture<DropBox<A>, F>,
}

fn _assert() {
    fn _assert_send<T: Send>() {}
    _assert_send::<LoopData<()>>();
}

impl<F, A> Future for AddLoopData<F, A>
    where F: FnOnce() -> A + Send + 'static,
          A: 'static,
{
    type Item = LoopData<A>;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<LoopData<A>, io::Error> {
        let ret = self.inner.poll(|_lp, f| {
            Ok(DropBox::new(f()))
        }, |f, slot| {
            Message::Run(Box::new(move || {
                slot.try_produce(Ok(DropBox::new(f()))).ok()
                    .expect("add loop data try_produce intereference");
            }))
        });

        ret.map(|data| {
            LoopData {
                data: data,
                handle: self.inner.loop_handle.clone(),
            }
        })
    }
}

impl<A: 'static> LoopData<A> {
    /// Gets a shared reference to the underlying data in this handle.
    ///
    /// Returns `None` if it is not called from the event loop thread that this
    /// `LoopData<A>` is associated with, or `Some` with a reference to the data
    /// if we are indeed on the event loop thread.
    pub fn get(&self) -> Option<&A> {
        self.data.get()
    }

    /// Gets a mutable reference to the underlying data in this handle.
    ///
    /// Returns `None` if it is not called from the event loop thread that this
    /// `LoopData<A>` is associated with, or `Some` with a reference to the data
    /// if we are indeed on the event loop thread.
    pub fn get_mut(&mut self) -> Option<&mut A> {
        self.data.get_mut()
    }

    /// Acquire the executor associated with the thread that owns this
    /// `LoopData<A>`'s data.
    ///
    /// If the `get` and `get_mut` functions above return `None`, then this data
    /// is being polled on the wrong thread to access the data, and to make
    /// progress a future may need to migrate to the actual thread which owns
    /// the relevant data.
    ///
    /// This executor can in turn be passed to `Task::poll_on`, which will then
    /// move the entire future to be polled on the right thread.
    pub fn executor(&self) -> Arc<Executor> {
        self.handle.tx.clone()
    }

    /// Returns a reference to the handle that this data is bound to.
    pub fn loop_handle(&self) -> &LoopHandle {
        &self.handle
    }
}

impl<A: Future> Future for LoopData<A> {
    type Item = A::Item;
    type Error = A::Error;

    fn poll(&mut self) -> Poll<A::Item, A::Error> {
        // If we're on the right thread, then we can proceed. Otherwise we need
        // to go and get polled on the right thread.
        if let Some(inner) = self.get_mut() {
            return inner.poll()
        }
        task::poll_on(self.executor());
        Poll::NotReady
    }
}

impl<A: 'static> Drop for LoopData<A> {
    fn drop(&mut self) {
        // The `DropBox` we store internally will cause a memory leak if it's
        // dropped on the wrong thread. While necessary for safety, we don't
        // actually want a memory leak, so for all normal circumstances we take
        // out the `DropBox<A>` as a `DropBox<MyDrop>` and then we send it off
        // to the event loop.
        //
        // TODO: possible optimization is to do none of this if we're on the
        //       event loop thread itself
        if let Some(data) = self.data.take() {
            self.handle.send(Message::Drop(data));
        }
    }
}

/// A curious inner module with one `unsafe` keyword, yet quite an important
/// one!
///
/// The purpose of this module is to define a type, `DropBox<A>`, which is able
/// to be sent across thread event when the underlying data `A` is itself not
/// sendable across threads. This is then in turn used to build up the
/// `LoopData` abstraction above.
///
/// A `DropBox` currently contains two major components, an identification of
/// the thread that it originated from as well as the data itself. Right now the
/// data is stored in a `Box` as we'll transition between it and `Box<MyDrop>`,
/// but this is perhaps optimizable.
///
/// The `DropBox<A>` itself only provides a few safe methods, all of which are
/// safe to call from any thread. Access to the underlying data is only granted
/// if we're on the right thread, and otherwise the methods don't access the
/// data itself.
///
/// Finally, one crucial piece, if the data is dropped it may run code that
/// assumes it's on the original thread. For this reason we have to be sure that
/// the data is only dropped on the originating thread itself. It's currently
/// the job of the outer `LoopData` to ensure that a `DropBox` is dropped on the
/// right thread, so we don't attempt to perform any communication in this
/// `Drop` implementation. Instead, if a `DropBox` is dropped on the wrong
/// thread, it simply leaks its contents.
///
/// All that's really just a lot of words in an attempt to justify the `unsafe`
/// impl of `Send` below. The idea is that the data is only ever accessed on the
/// originating thread, even during `Drop`.
///
/// Note that this is a private module to have a visibility boundary around the
/// unsafe internals. Although there's not any unsafe blocks here, the code
/// itself is quite unsafe as it has to make sure that the data is dropped in
/// the right place, if ever.
mod dropbox {
    use std::mem;
    use super::{CURRENT_LOOP, LoopPin};

    pub struct DropBox<A: ?Sized> {
        id: usize,
        inner: Option<Box<A>>,
    }

    // We can be sent across threads due to the comment above
    unsafe impl<A: ?Sized> Send for DropBox<A> {}

    // We can also be shared across threads just fine as we'll only ever get a
    // reference on at most one thread, regardless of `A`.
    unsafe impl<A: ?Sized> Sync for DropBox<A> {}

    pub trait MyDrop {}
    impl<T: ?Sized> MyDrop for T {}

    impl<A> DropBox<A> {
        /// Creates a new `DropBox` pinned to the current threads.
        ///
        /// Will panic if `CURRENT_LOOP` isn't set.
        pub fn new(a: A) -> DropBox<A> {
            DropBox {
                id: CURRENT_LOOP.with(|lp| lp.id),
                inner: Some(Box::new(a)),
            }
        }

        /// Creates a new `DropBox` pinned to the thread of `LoopPin`.
        pub fn new_on(a: A, lp: &LoopPin) -> DropBox<A> {
            DropBox {
                id: lp.handle.id,
                inner: Some(Box::new(a)),
            }
        }

        /// Consumes the contents of this `DropBox<A>`, returning a new
        /// `DropBox<MyDrop>`.
        ///
        /// This is just intended to be a simple and cheap conversion, should
        /// almost always return `Some`.
        pub fn take<'a>(&mut self) -> Option<DropBox<MyDrop + 'a>>
            where A: 'a
        {
            self.inner.take().map(|d| {
                DropBox { id: self.id, inner: Some(d as Box<MyDrop + 'a>) }
            })
        }
    }

    impl<A: ?Sized> DropBox<A> {
        /// Returns a shared reference to the data if we're on the right
        /// thread.
        pub fn get(&self) -> Option<&A> {
            if CURRENT_LOOP.is_set() {
                CURRENT_LOOP.with(|lp| {
                    if lp.id == self.id {
                        self.inner.as_ref().map(|b| &**b)
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        }

        /// Returns a mutable reference to the data if we're on the right
        /// thread.
        pub fn get_mut(&mut self) -> Option<&mut A> {
            if CURRENT_LOOP.is_set() {
                CURRENT_LOOP.with(move |lp| {
                    if lp.id == self.id {
                        self.inner.as_mut().map(|b| &mut **b)
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        }
    }

    impl<A: ?Sized> Drop for DropBox<A> {
        fn drop(&mut self) {
            // Try our safe accessor first, and if it works then we know that
            // we're on the right thread. In that case we can simply drop as
            // usual.
            if let Some(a) = self.get_mut().take() {
                return drop(a)
            }

            // If we're on the wrong thread but we actually have some data, then
            // something in theory horrible has gone awry. Prevent memory safety
            // issues by forgetting the data and then also warn about this odd
            // event.
            if let Some(data) = self.inner.take() {
                mem::forget(data);
                warn!("forgetting some data on an event loop");
            }
        }
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

impl<E> Source<E> {
    /// Creates a new `Source` wrapping the provided source of events.
    pub fn new(e: E) -> Source<E> {
        Source {
            readiness: AtomicUsize::new(0),
            io: e,
        }
    }
}

impl<E: ?Sized> Source<E> {
    /// Consumes the last readiness notification that this source received.
    ///
    /// Currently sources receive readiness notifications on an edge-basis. That
    /// is, once you receive a notification that an object can be read, you
    /// won't receive any more notifications until all of that data has been
    /// read.
    ///
    /// The event loop will fill in this information and then inform futures
    /// that they're ready to go with the `schedule` method, and then the `poll`
    /// method can use this to figure out what happened.
    pub fn take_readiness(&self) -> usize {
        self.readiness.swap(0, Ordering::SeqCst)
    }

    /// Gets access to the underlying I/O object.
    pub fn io(&self) -> &E {
        &self.io
    }
}

impl Executor for MioSender {
    fn execute_boxed(&self, callback: Box<ExecuteCallback>) {
        self.inner.send(Message::Run(callback))
            .expect("error sending a message to the event loop")
    }
}
