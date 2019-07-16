use super::Borrow;
use tokio_executor::park::Unpark;
use tokio_executor::Enter;

use futures::executor::{self, NotifyHandle, Spawn, UnsafeNotify};
use futures::{Async, Future};

use std::cell::UnsafeCell;
use std::fmt::{self, Debug};
use std::marker::PhantomData;
use std::mem;
use std::ptr;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release, SeqCst};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize};
use std::sync::{Arc, Weak};
use std::thread;
use std::usize;

/// A generic task-aware scheduler.
///
/// This is used both by `FuturesUnordered` and the current-thread executor.
pub struct Scheduler<U> {
    inner: Arc<Inner<U>>,
    nodes: List<U>,
}

pub struct Notify<'a, U: 'a>(&'a Arc<Node<U>>);

// A linked-list of nodes
struct List<U> {
    len: usize,
    head: *const Node<U>,
    tail: *const Node<U>,
}

// Scheduler is implemented using two linked lists. The first linked list tracks
// all items managed by a `Scheduler`. This list is stored on the `Scheduler`
// struct and is **not** thread safe. The second linked list is an
// implementation of the intrusive MPSC queue algorithm described by
// 1024cores.net and is stored on `Inner`. This linked list can push items to
// the back concurrently but only one consumer may pop from the front. To
// enforce this requirement, all popping will be performed via fns on
// `Scheduler` that take `&mut self`.
//
// When a item is submitted to the set a node is allocated and inserted in
// both linked lists. This means that all insertion operations **must** be
// originated from `Scheduler` with `&mut self` The next call to `tick` will
// (eventually) see this node and call `poll` on the item.
//
// Nodes are wrapped in `Arc` cells which manage the lifetime of the node.
// However, `Arc` handles are sometimes cast to `*const Node` pointers.
// Specifically, when a node is stored in at least one of the two lists
// described above, this represents a logical `Arc` handle. This is how
// `Scheduler` maintains its reference to all nodes it manages. Each
// `NotifyHandle` instance is an `Arc<Node>` as well.
//
// When `Scheduler` drops, it clears the linked list of all nodes that it
// manages. When doing so, it must attempt to decrement the reference count (by
// dropping an Arc handle). However, it can **only** decrement the reference
// count if the node is not currently stored in the mpsc channel. If the node
// **is** "queued" in the mpsc channel, then the arc reference count cannot be
// decremented. Once the node is popped from the mpsc channel, then the final
// arc reference count can be decremented, thus freeing the node.

struct Inner<U> {
    // Thread unpark handle
    unpark: U,

    // Tick number
    tick_num: AtomicUsize,

    // Head/tail of the readiness queue
    head_readiness: AtomicPtr<Node<U>>,
    tail_readiness: UnsafeCell<*const Node<U>>,

    // Used as part of the mpsc queue algorithm
    stub: Arc<Node<U>>,
}

unsafe impl<U: Sync + Send> Send for Inner<U> {}
unsafe impl<U: Sync + Send> Sync for Inner<U> {}

impl<U: Unpark> executor::Notify for Inner<U> {
    fn notify(&self, _: usize) {
        self.unpark.unpark();
    }
}

struct Node<U> {
    // The item
    item: UnsafeCell<Option<Task>>,

    // The tick at which this node was notified
    notified_at: AtomicUsize,

    // Next pointer for linked list tracking all active nodes
    next_all: UnsafeCell<*const Node<U>>,

    // Previous node in linked list tracking all active nodes
    prev_all: UnsafeCell<*const Node<U>>,

    // Next pointer in readiness queue
    next_readiness: AtomicPtr<Node<U>>,

    // Whether or not this node is currently in the mpsc queue.
    queued: AtomicBool,

    // Queue that we'll be enqueued to when notified
    queue: Weak<Inner<U>>,
}

/// Returned by `Inner::dequeue`, representing either a dequeue success (with
/// the dequeued node), an empty list, or an inconsistent state.
///
/// The inconsistent state is described in more detail at [1024cores], but
/// roughly indicates that a node will be ready to dequeue sometime shortly in
/// the future and the caller should try again soon.
///
/// [1024cores]: http://www.1024cores.net/home/lock-free-algorithms/queues/intrusive-mpsc-node-based-queue
enum Dequeue<U> {
    Data(*const Node<U>),
    Empty,
    Yield,
    Inconsistent,
}

/// Wraps a spawned boxed future
struct Task(Spawn<Box<dyn Future<Item = (), Error = ()>>>);

/// A task that is scheduled. `turn` must be called
pub struct Scheduled<'a, U: 'a> {
    task: &'a mut Task,
    notify: &'a Notify<'a, U>,
    done: &'a mut bool,
}

impl<U> Scheduler<U>
where
    U: Unpark,
{
    /// Constructs a new, empty `Scheduler`
    ///
    /// The returned `Scheduler` does not contain any items and, in this
    /// state, `Scheduler::poll` will return `Ok(Async::Ready(None))`.
    pub fn new(unpark: U) -> Self {
        let stub = Arc::new(Node {
            item: UnsafeCell::new(None),
            notified_at: AtomicUsize::new(0),
            next_all: UnsafeCell::new(ptr::null()),
            prev_all: UnsafeCell::new(ptr::null()),
            next_readiness: AtomicPtr::new(ptr::null_mut()),
            queued: AtomicBool::new(true),
            queue: Weak::new(),
        });
        let stub_ptr = &*stub as *const Node<U>;
        let inner = Arc::new(Inner {
            unpark,
            tick_num: AtomicUsize::new(0),
            head_readiness: AtomicPtr::new(stub_ptr as *mut _),
            tail_readiness: UnsafeCell::new(stub_ptr),
            stub: stub,
        });

        Scheduler {
            inner: inner,
            nodes: List::new(),
        }
    }

    pub fn notify(&self) -> NotifyHandle {
        self.inner.clone().into()
    }

    pub fn schedule(&mut self, item: Box<dyn Future<Item = (), Error = ()>>) {
        // Get the current scheduler tick
        let tick_num = self.inner.tick_num.load(SeqCst);

        let node = Arc::new(Node {
            item: UnsafeCell::new(Some(Task::new(item))),
            notified_at: AtomicUsize::new(tick_num),
            next_all: UnsafeCell::new(ptr::null_mut()),
            prev_all: UnsafeCell::new(ptr::null_mut()),
            next_readiness: AtomicPtr::new(ptr::null_mut()),
            queued: AtomicBool::new(true),
            queue: Arc::downgrade(&self.inner),
        });

        // Right now our node has a strong reference count of 1. We transfer
        // ownership of this reference count to our internal linked list
        // and we'll reclaim ownership through the `unlink` function below.
        let ptr = self.nodes.push_back(node);

        // We'll need to get the item "into the system" to start tracking it,
        // e.g. getting its unpark notifications going to us tracking which
        // items are ready. To do that we unconditionally enqueue it for
        // polling here.
        self.inner.enqueue(ptr);
    }

    /// Returns `true` if there are currently any pending futures
    pub fn has_pending_futures(&mut self) -> bool {
        // See function definition for why the unsafe is needed and
        // correctly used here
        unsafe { self.inner.has_pending_futures() }
    }

    /// Advance the scheduler state, returning `true` if any futures were
    /// processed.
    ///
    /// This function should be called whenever the caller is notified via a
    /// wakeup.
    pub fn tick(&mut self, eid: u64, enter: &mut Enter, num_futures: &AtomicUsize) -> bool {
        let mut ret = false;
        let tick = self.inner.tick_num.fetch_add(1, SeqCst).wrapping_add(1);

        loop {
            let node = match unsafe { self.inner.dequeue(Some(tick)) } {
                Dequeue::Empty => {
                    return ret;
                }
                Dequeue::Yield => {
                    self.inner.unpark.unpark();
                    return ret;
                }
                Dequeue::Inconsistent => {
                    thread::yield_now();
                    continue;
                }
                Dequeue::Data(node) => node,
            };

            ret = true;

            debug_assert!(node != self.inner.stub());

            unsafe {
                if (*(*node).item.get()).is_none() {
                    // The node has already been released. However, while it was
                    // being released, another thread notified it, which
                    // resulted in it getting pushed into the mpsc channel.
                    //
                    // In this case, we just decrement the ref count.
                    let node = ptr2arc(node);
                    assert!((*node.next_all.get()).is_null());
                    assert!((*node.prev_all.get()).is_null());
                    continue;
                };

                // We're going to need to be very careful if the `poll`
                // function below panics. We need to (a) not leak memory and
                // (b) ensure that we still don't have any use-after-frees. To
                // manage this we do a few things:
                //
                // * This "bomb" here will call `release_node` if dropped
                //   abnormally. That way we'll be sure the memory management
                //   of the `node` is managed correctly.
                //
                // * We unlink the node from our internal queue to preemptively
                //   assume is is complete (will return Ready or panic), in
                //   which case we'll want to discard it regardless.
                //
                struct Bomb<'a, U: Unpark + 'a> {
                    borrow: &'a mut Borrow<'a, U>,
                    enter: &'a mut Enter,
                    node: Option<Arc<Node<U>>>,
                }

                impl<'a, U: Unpark> Drop for Bomb<'a, U> {
                    fn drop(&mut self) {
                        if let Some(node) = self.node.take() {
                            self.borrow.enter(self.enter, || release_node(node))
                        }
                    }
                }

                let node = self.nodes.remove(node);

                let mut borrow = Borrow {
                    id: eid,
                    scheduler: self,
                    num_futures,
                };

                let mut bomb = Bomb {
                    node: Some(node),
                    enter: enter,
                    borrow: &mut borrow,
                };

                let mut done = false;

                // Now that the bomb holds the node, create a new scope. This
                // scope ensures that the borrow will go out of scope before we
                // mutate the node pointer in `bomb` again
                {
                    let node = bomb.node.as_ref().unwrap();

                    // Get a reference to the inner future. We already ensured
                    // that the item `is_some`.
                    let item = (*node.item.get()).as_mut().unwrap();

                    // Unset queued flag... this must be done before
                    // polling. This ensures that the item gets
                    // rescheduled if it is notified **during** a call
                    // to `poll`.
                    let prev = (*node).queued.swap(false, SeqCst);
                    assert!(prev);

                    // Poll the underlying item with the appropriate `notify`
                    // implementation. This is where a large bit of the unsafety
                    // starts to stem from internally. The `notify` instance itself
                    // is basically just our `Arc<Node>` and tracks the mpsc
                    // queue of ready items.
                    //
                    // Critically though `Node` won't actually access `Task`, the
                    // item, while it's floating around inside of `Task`
                    // instances. These structs will basically just use `T` to size
                    // the internal allocation, appropriately accessing fields and
                    // deallocating the node if need be.
                    let borrow = &mut *bomb.borrow;
                    let enter = &mut *bomb.enter;
                    let notify = Notify(bomb.node.as_ref().unwrap());

                    let mut scheduled = Scheduled {
                        task: item,
                        notify: &notify,
                        done: &mut done,
                    };

                    if borrow.enter(enter, || scheduled.tick()) {
                        // we have a borrow of the Runtime, so we know it's not shut down
                        borrow.num_futures.fetch_sub(2, SeqCst);
                    }
                }

                if !done {
                    // The future is not done, push it back into the "all
                    // node" list.
                    let node = bomb.node.take().unwrap();
                    bomb.borrow.scheduler.nodes.push_back(node);
                }
            }
        }
    }
}

impl<'a, U: Unpark> Scheduled<'a, U> {
    /// Polls the task, returns `true` if the task has completed.
    pub fn tick(&mut self) -> bool {
        // Tick the future
        let ret = match self.task.0.poll_future_notify(self.notify, 0) {
            Ok(Async::Ready(_)) | Err(_) => true,
            Ok(Async::NotReady) => false,
        };

        *self.done = ret;
        ret
    }
}

impl Task {
    pub fn new(future: Box<dyn Future<Item = (), Error = ()> + 'static>) -> Self {
        Task(executor::spawn(future))
    }
}

impl fmt::Debug for Task {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Task").finish()
    }
}

fn release_node<U>(node: Arc<Node<U>>) {
    // The item is done, try to reset the queued flag. This will prevent
    // `notify` from doing any work in the item
    let prev = node.queued.swap(true, SeqCst);

    // Drop the item, even if it hasn't finished yet. This is safe
    // because we're dropping the item on the thread that owns
    // `Scheduler`, which correctly tracks T's lifetimes and such.
    unsafe {
        drop((*node.item.get()).take());
    }

    // If the queued flag was previously set then it means that this node
    // is still in our internal mpsc queue. We then transfer ownership
    // of our reference count to the mpsc queue, and it'll come along and
    // free it later, noticing that the item is `None`.
    //
    // If, however, the queued flag was *not* set then we're safe to
    // release our reference count on the internal node. The queued flag
    // was set above so all item `enqueue` operations will not actually
    // enqueue the node, so our node will never see the mpsc queue again.
    // The node itself will be deallocated once all reference counts have
    // been dropped by the various owning tasks elsewhere.
    if prev {
        mem::forget(node);
    }
}

impl<U> Debug for Scheduler<U> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Scheduler {{ ... }}")
    }
}

impl<U> Drop for Scheduler<U> {
    fn drop(&mut self) {
        // When a `Scheduler` is dropped we want to drop all items associated
        // with it. At the same time though there may be tons of `Task` handles
        // flying around which contain `Node` references inside them. We'll
        // let those naturally get deallocated when the `Task` itself goes out
        // of scope or gets notified.
        while let Some(node) = self.nodes.pop_front() {
            release_node(node);
        }

        // Note that at this point we could still have a bunch of nodes in the
        // mpsc queue. None of those nodes, however, have items associated
        // with them so they're safe to destroy on any thread. At this point
        // the `Scheduler` struct, the owner of the one strong reference
        // to `Inner` will drop the strong reference. At that point
        // whichever thread releases the strong refcount last (be it this
        // thread or some other thread as part of an `upgrade`) will clear out
        // the mpsc queue and free all remaining nodes.
        //
        // While that freeing operation isn't guaranteed to happen here, it's
        // guaranteed to happen "promptly" as no more "blocking work" will
        // happen while there's a strong refcount held.
    }
}

impl<U> Inner<U> {
    /// The enqueue function from the 1024cores intrusive MPSC queue algorithm.
    fn enqueue(&self, node: *const Node<U>) {
        unsafe {
            debug_assert!((*node).queued.load(Relaxed));

            // This action does not require any coordination
            (*node).next_readiness.store(ptr::null_mut(), Relaxed);

            // Note that these atomic orderings come from 1024cores
            let node = node as *mut _;
            let prev = self.head_readiness.swap(node, AcqRel);
            (*prev).next_readiness.store(node, Release);
        }
    }

    /// Returns `true` if there are currently any pending futures
    ///
    /// See `dequeue` for an explanation why this function is unsafe.
    unsafe fn has_pending_futures(&self) -> bool {
        let tail = *self.tail_readiness.get();
        let next = (*tail).next_readiness.load(Acquire);

        if tail == self.stub() {
            if next.is_null() {
                return false;
            }
        }

        true
    }

    /// The dequeue function from the 1024cores intrusive MPSC queue algorithm
    ///
    /// Note that this unsafe as it required mutual exclusion (only one thread
    /// can call this) to be guaranteed elsewhere.
    unsafe fn dequeue(&self, tick: Option<usize>) -> Dequeue<U> {
        let mut tail = *self.tail_readiness.get();
        let mut next = (*tail).next_readiness.load(Acquire);

        if tail == self.stub() {
            if next.is_null() {
                return Dequeue::Empty;
            }

            *self.tail_readiness.get() = next;
            tail = next;
            next = (*next).next_readiness.load(Acquire);
        }

        if let Some(tick) = tick {
            let actual = (*tail).notified_at.load(SeqCst);

            // Only dequeue if the node was not scheduled during the current
            // tick.
            if actual == tick {
                // Only doing the check above **should** be enough in
                // practice. However, technically there is a potential for
                // deadlocking if there are `usize::MAX` ticks while the thread
                // scheduling the task is frozen.
                //
                // If, for some reason, this is not enough, calling `unpark`
                // here will resolve the issue.
                return Dequeue::Yield;
            }
        }

        if !next.is_null() {
            *self.tail_readiness.get() = next;
            debug_assert!(tail != self.stub());
            return Dequeue::Data(tail);
        }

        if self.head_readiness.load(Acquire) as *const _ != tail {
            return Dequeue::Inconsistent;
        }

        self.enqueue(self.stub());

        next = (*tail).next_readiness.load(Acquire);

        if !next.is_null() {
            *self.tail_readiness.get() = next;
            return Dequeue::Data(tail);
        }

        Dequeue::Inconsistent
    }

    fn stub(&self) -> *const Node<U> {
        &*self.stub
    }
}

impl<U> Drop for Inner<U> {
    fn drop(&mut self) {
        // Once we're in the destructor for `Inner` we need to clear out the
        // mpsc queue of nodes if there's anything left in there.
        //
        // Note that each node has a strong reference count associated with it
        // which is owned by the mpsc queue. All nodes should have had their
        // items dropped already by the `Scheduler` destructor above,
        // so we're just pulling out nodes and dropping their refcounts.
        unsafe {
            loop {
                match self.dequeue(None) {
                    Dequeue::Empty => break,
                    Dequeue::Yield => unreachable!(),
                    Dequeue::Inconsistent => abort("inconsistent in drop"),
                    Dequeue::Data(ptr) => drop(ptr2arc(ptr)),
                }
            }
        }
    }
}

impl<U> List<U> {
    fn new() -> Self {
        List {
            len: 0,
            head: ptr::null_mut(),
            tail: ptr::null_mut(),
        }
    }

    /// Appends an element to the back of the list
    fn push_back(&mut self, node: Arc<Node<U>>) -> *const Node<U> {
        let ptr = arc2ptr(node);

        unsafe {
            // Point to the current last node in the list
            *(*ptr).prev_all.get() = self.tail;
            *(*ptr).next_all.get() = ptr::null_mut();

            if !self.tail.is_null() {
                *(*self.tail).next_all.get() = ptr;
                self.tail = ptr;
            } else {
                // This is the first node
                self.tail = ptr;
                self.head = ptr;
            }
        }

        self.len += 1;

        return ptr;
    }

    /// Pop an element from the front of the list
    fn pop_front(&mut self) -> Option<Arc<Node<U>>> {
        if self.head.is_null() {
            // The list is empty
            return None;
        }

        self.len -= 1;

        unsafe {
            // Convert the ptr to Arc<_>
            let node = ptr2arc(self.head);

            // Update the head pointer
            self.head = *node.next_all.get();

            // If the pointer is null, then the list is empty
            if self.head.is_null() {
                self.tail = ptr::null_mut();
            } else {
                *(*self.head).prev_all.get() = ptr::null_mut();
            }

            Some(node)
        }
    }

    /// Remove a specific node
    unsafe fn remove(&mut self, node: *const Node<U>) -> Arc<Node<U>> {
        let node = ptr2arc(node);
        let next = *node.next_all.get();
        let prev = *node.prev_all.get();
        *node.next_all.get() = ptr::null_mut();
        *node.prev_all.get() = ptr::null_mut();

        if !next.is_null() {
            *(*next).prev_all.get() = prev;
        } else {
            self.tail = prev;
        }

        if !prev.is_null() {
            *(*prev).next_all.get() = next;
        } else {
            self.head = next;
        }

        self.len -= 1;

        return node;
    }
}

impl<'a, U> Clone for Notify<'a, U> {
    fn clone(&self) -> Self {
        Notify(self.0)
    }
}

impl<'a, U> fmt::Debug for Notify<'a, U> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Notify").finish()
    }
}

impl<'a, U: Unpark> From<Notify<'a, U>> for NotifyHandle {
    fn from(handle: Notify<'a, U>) -> NotifyHandle {
        unsafe {
            let ptr = handle.0.clone();
            let ptr = mem::transmute::<Arc<Node<U>>, *mut ArcNode<U>>(ptr);
            NotifyHandle::new(hide_lt(ptr))
        }
    }
}

struct ArcNode<U>(PhantomData<U>);

// We should never touch `Task` on any thread other than the one owning
// `Scheduler`, so this should be a safe operation.
unsafe impl<U: Sync + Send> Send for ArcNode<U> {}
unsafe impl<U: Sync + Send> Sync for ArcNode<U> {}

impl<U: Unpark> executor::Notify for ArcNode<U> {
    fn notify(&self, _id: usize) {
        unsafe {
            let me: *const ArcNode<U> = self;
            let me: *const *const ArcNode<U> = &me;
            let me = me as *const Arc<Node<U>>;
            Node::notify(&*me)
        }
    }
}

unsafe impl<U: Unpark> UnsafeNotify for ArcNode<U> {
    unsafe fn clone_raw(&self) -> NotifyHandle {
        let me: *const ArcNode<U> = self;
        let me: *const *const ArcNode<U> = &me;
        let me = &*(me as *const Arc<Node<U>>);
        Notify(me).into()
    }

    unsafe fn drop_raw(&self) {
        let mut me: *const ArcNode<U> = self;
        let me = &mut me as *mut *const ArcNode<U> as *mut Arc<Node<U>>;
        ptr::drop_in_place(me);
    }
}

unsafe fn hide_lt<U: Unpark>(p: *mut ArcNode<U>) -> *mut dyn UnsafeNotify {
    mem::transmute(p as *mut dyn UnsafeNotify)
}

impl<U: Unpark> Node<U> {
    fn notify(me: &Arc<Node<U>>) {
        let inner = match me.queue.upgrade() {
            Some(inner) => inner,
            None => return,
        };

        // It's our job to notify the node that it's ready to get polled,
        // meaning that we need to enqueue it into the readiness queue. To
        // do this we flag that we're ready to be queued, and if successful
        // we then do the literal queueing operation, ensuring that we're
        // only queued once.
        //
        // Once the node is inserted we be sure to notify the parent task,
        // as it'll want to come along and pick up our node now.
        //
        // Note that we don't change the reference count of the node here,
        // we're just enqueueing the raw pointer. The `Scheduler`
        // implementation guarantees that if we set the `queued` flag true that
        // there's a reference count held by the main `Scheduler` queue
        // still.
        let prev = me.queued.swap(true, SeqCst);
        if !prev {
            // Get the current scheduler tick
            let tick_num = inner.tick_num.load(SeqCst);
            me.notified_at.store(tick_num, SeqCst);

            inner.enqueue(&**me);
            inner.unpark.unpark();
        }
    }
}

impl<U> Drop for Node<U> {
    fn drop(&mut self) {
        // Currently a `Node` is sent across all threads for any lifetime,
        // regardless of `T`. This means that for memory safety we can't
        // actually touch `T` at any time except when we have a reference to the
        // `Scheduler` itself.
        //
        // Consequently it *should* be the case that we always drop items from
        // the `Scheduler` instance, but this is a bomb in place to catch
        // any bugs in that logic.
        unsafe {
            if (*self.item.get()).is_some() {
                abort("item still here when dropping");
            }
        }
    }
}

fn arc2ptr<T>(ptr: Arc<T>) -> *const T {
    let addr = &*ptr as *const T;
    mem::forget(ptr);
    return addr;
}

unsafe fn ptr2arc<T>(ptr: *const T) -> Arc<T> {
    let anchor = mem::transmute::<usize, Arc<T>>(0x10);
    let addr = &*anchor as *const T;
    mem::forget(anchor);
    let offset = addr as isize - 0x10;
    mem::transmute::<isize, Arc<T>>(ptr as isize - offset)
}

fn abort(s: &str) -> ! {
    struct DoublePanic;

    impl Drop for DoublePanic {
        fn drop(&mut self) {
            panic!("panicking twice to abort the program");
        }
    }

    let _bomb = DoublePanic;
    panic!("{}", s);
}
