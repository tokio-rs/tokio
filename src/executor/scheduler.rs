//! An unbounded set of futures.

use super::sleep::Wakeup;

use futures::Async;
use futures::executor::{self, UnsafeNotify, NotifyHandle};

use std::cell::UnsafeCell;
use std::fmt::{self, Debug};
use std::marker::PhantomData;
use std::mem;
use std::ptr;
use std::sync::atomic::Ordering::{Relaxed, SeqCst, Acquire, Release, AcqRel};
use std::sync::atomic::{AtomicPtr, AtomicBool};
use std::sync::{Arc, Weak};
use std::usize;

/// A generic task-aware scheduler.
///
/// This is used both by `FuturesUnordered` and the current-thread executor.
pub struct Scheduler<T, W> {
    inner: Arc<Inner<T, W>>,
    nodes: List<T, W>,
}

/// Schedule new futures
pub trait Schedule<T> {
    /// Schedule a new future.
    fn schedule(&mut self, item: T);
}

pub struct Notify<'a, T: 'a, W: 'a>(&'a Arc<Node<T, W>>);

// A linked-list of nodes
struct List<T, W> {
    len: usize,
    head: *const Node<T, W>,
    tail: *const Node<T, W>,
}

unsafe impl<T: Send, W: Wakeup> Send for Scheduler<T, W> {}
unsafe impl<T: Sync, W: Wakeup> Sync for Scheduler<T, W> {}

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
// `NotifyHande` instance is an `Arc<Node>` as well.
//
// When `Scheduler` drops, it clears the linked list of all nodes that it
// manages. When doing so, it must attempt to decrement the reference count (by
// dropping an Arc handle). However, it can **only** decrement the reference
// count if the node is not currently stored in the mpsc channel. If the node
// **is** "queued" in the mpsc channel, then the arc reference count cannot be
// decremented. Once the node is popped from the mpsc channel, then the final
// arc reference count can be decremented, thus freeing the node.

#[allow(missing_debug_implementations)]
struct Inner<T, W> {
    // The task using `Scheduler`.
    wakeup: W,

    // Head/tail of the readiness queue
    head_readiness: AtomicPtr<Node<T, W>>,
    tail_readiness: UnsafeCell<*const Node<T, W>>,

    // Used as part of the MPSC queue algorithm
    stub: Arc<Node<T, W>>,
}

struct Node<T, W> {
    // The item
    item: UnsafeCell<Option<T>>,

    // Next pointer for linked list tracking all active nodes
    next_all: UnsafeCell<*const Node<T, W>>,

    // Previous node in linked list tracking all active nodes
    prev_all: UnsafeCell<*const Node<T, W>>,

    // Next pointer in readiness queue
    next_readiness: AtomicPtr<Node<T, W>>,

    // Whether or not this node is currently in the mpsc queue.
    queued: AtomicBool,

    // Queue that we'll be enqueued to when notified
    queue: Weak<Inner<T, W>>,
}

/// Returned by the `Scheduler::tick` function, allowing the caller to decide
/// what action to take next.
pub enum Tick<T> {
    Data(T),
    Empty,
    Inconsistent,
}

/// Returned by `Inner::dequeue`, representing either a dequeue success (with
/// the dequeued node), an empty list, or an inconsistent state.
///
/// The inconsistent state is described in more detail at [1024cores], but
/// roughly indicates that a node will be ready to dequeue sometime shortly in
/// the future and the caller should try again soon.
///
/// [1024cores]: http://www.1024cores.net/home/lock-free-algorithms/queues/intrusive-mpsc-node-based-queue
enum Dequeue<T, W> {
    Data(*const Node<T, W>),
    Empty,
    Inconsistent,
}

impl<T, W> Scheduler<T, W>
where W: Wakeup,
{
    /// Constructs a new, empty `Scheduler`
    ///
    /// The returned `Scheduler` does not contain any items and, in this
    /// state, `Scheduler::poll` will return `Ok(Async::Ready(None))`.
    pub fn new(wakeup: W) -> Self {
        let stub = Arc::new(Node {
            item: UnsafeCell::new(None),
            next_all: UnsafeCell::new(ptr::null()),
            prev_all: UnsafeCell::new(ptr::null()),
            next_readiness: AtomicPtr::new(ptr::null_mut()),
            queued: AtomicBool::new(true),
            queue: Weak::new(),
        });
        let stub_ptr = &*stub as *const Node<T, W>;
        let inner = Arc::new(Inner {
            wakeup: wakeup,
            head_readiness: AtomicPtr::new(stub_ptr as *mut _),
            tail_readiness: UnsafeCell::new(stub_ptr),
            stub: stub,
        });

        Scheduler {
            inner: inner,
            nodes: List::new(),
        }
    }
}

impl<T, W: Wakeup> Scheduler<T, W> {
    /// Advance the scheduler state.
    ///
    /// This function should be called whenever the caller is notified via a
    /// wakeup.
    pub fn tick<F, R>(&mut self, mut f: F) -> Tick<R>
    where F: FnMut(&mut Self, &mut T, &Notify<T, W>) -> Async<R>
    {
        loop {
            let node = match unsafe { self.inner.dequeue() } {
                Dequeue::Empty => {
                    return Tick::Empty;
                }
                Dequeue::Inconsistent => {
                    return Tick::Inconsistent;
                }
                Dequeue::Data(node) => node,
            };

            debug_assert!(node != self.inner.stub());

            unsafe {
                if (*(*node).item.get()).is_none() {
                    // The node has already been released. However, while it was
                    // being released, another thread notified it, which
                    // resulted in it getting pushed into the mpsc channel.
                    //
                    // In this case, we just dec the ref count.
                    let node = ptr2arc(node);
                    assert!((*node.next_all.get()).is_null());
                    assert!((*node.prev_all.get()).is_null());
                    continue
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
                struct Bomb<'a, T: 'a, W: 'a> {
                    queue: &'a mut Scheduler<T, W>,
                    node: Option<Arc<Node<T, W>>>,
                }

                impl<'a, T, W> Drop for Bomb<'a, T, W> {
                    fn drop(&mut self) {
                        if let Some(node) = self.node.take() {
                            release_node(node);
                        }
                    }
                }

                let mut bomb = Bomb {
                    node: Some(self.nodes.remove(node)),
                    queue: self,
                };

                // Now that the bomb holds the node, create a new scope. This
                // scope ensures that the borrow will go out of scope before we
                // mutate the node pointer in `bomb` again
                let res = {
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
                    // is basically just our `Arc<Node<T>>` and tracks the mpsc
                    // queue of ready items.
                    //
                    // Critically though `Node<T>` won't actually access `T`, the
                    // item, while it's floating around inside of `Task`
                    // instances. These structs will basically just use `T` to size
                    // the internal allocation, appropriately accessing fields and
                    // deallocating the node if need be.
                    let queue = &mut *bomb.queue;
                    let notify = Notify(bomb.node.as_ref().unwrap());
                    f(queue, item, &notify)
                };

                let ret = match res {
                    Async::NotReady => {
                        // The future is not done, push it back into the "all
                        // node" list.
                        let node = bomb.node.take().unwrap();
                        bomb.queue.nodes.push_back(node);
                        continue;
                    }
                    Async::Ready(v) => {
                        // `bomb` will take care of unlinking and releasing the
                        // node.
                        Tick::Data(v)
                    }
                };

                return ret
            }
        }
    }
}

impl<T, W: Wakeup> Schedule<T> for Scheduler<T, W> {
    fn schedule(&mut self, item: T) {
        let node = Arc::new(Node {
            item: UnsafeCell::new(Some(item)),
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
}

fn release_node<T, W>(node: Arc<Node<T, W>>) {
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

impl<T: Debug, W: Debug> Debug for Scheduler<T, W> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Scheduler {{ ... }}")
    }
}

impl<T, W> Drop for Scheduler<T, W> {
    fn drop(&mut self) {
        // When a `Scheduler` is dropped we want to drop all items associated
        // with it. At the same time though there may be tons of `Task` handles
        // flying around which contain `Node<T>` references inside them. We'll
        // let those naturally get deallocated when the `Task` itself goes out
        // of scope or gets notified.
        while let Some(node) = self.nodes.pop_front() {
            release_node(node);
        }

        // Note that at this point we could still have a bunch of nodes in the
        // mpsc queue. None of those nodes, however, have items associated
        // with them so they're safe to destroy on any thread. At this point
        // the `Scheduler` struct, the owner of the one strong reference
        // to `Inner<T>` will drop the strong reference. At that point
        // whichever thread releases the strong refcount last (be it this
        // thread or some other thread as part of an `upgrade`) will clear out
        // the mpsc queue and free all remaining nodes.
        //
        // While that freeing operation isn't guaranteed to happen here, it's
        // guaranteed to happen "promptly" as no more "blocking work" will
        // happen while there's a strong refcount held.
    }
}

impl<T, W> Inner<T, W> {
    /// The enqueue function from the 1024cores intrusive MPSC queue algorithm.
    fn enqueue(&self, node: *const Node<T, W>) {
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

    /// The dequeue function from the 1024cores intrusive MPSC queue algorithm
    ///
    /// Note that this unsafe as it required mutual exclusion (only one thread
    /// can call this) to be guaranteed elsewhere.
    unsafe fn dequeue(&self) -> Dequeue<T, W> {
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

    fn stub(&self) -> *const Node<T, W> {
        &*self.stub
    }
}

impl<T, W> Drop for Inner<T, W> {
    fn drop(&mut self) {
        // Once we're in the destructor for `Inner<T, W>` we need to clear out the
        // mpsc queue of nodes if there's anything left in there.
        //
        // Note that each node has a strong reference count associated with it
        // which is owned by the mpsc queue. All nodes should have had their
        // items dropped already by the `Scheduler` destructor above,
        // so we're just pulling out nodes and dropping their refcounts.
        unsafe {
            loop {
                match self.dequeue() {
                    Dequeue::Empty => break,
                    Dequeue::Inconsistent => abort("inconsistent in drop"),
                    Dequeue::Data(ptr) => drop(ptr2arc(ptr)),
                }
            }
        }
    }
}

impl<T, W> List<T, W> {
    fn new() -> Self {
        List {
            len: 0,
            head: ptr::null_mut(),
            tail: ptr::null_mut(),
        }
    }

    /// Prepends an element to the back of the list
    fn push_back(&mut self, node: Arc<Node<T, W>>) -> *const Node<T, W> {
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

        return ptr
    }

    /// Pop an element from the front of the list
    fn pop_front(&mut self) -> Option<Arc<Node<T, W>>> {
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
    unsafe fn remove(&mut self, node: *const Node<T, W>) -> Arc<Node<T, W>> {
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

        return node
    }
}

impl<'a, T, W> Clone for Notify<'a, T, W> {
    fn clone(&self) -> Self {
        Notify(self.0)
    }
}

impl<'a, T: fmt::Debug, W: fmt::Debug> fmt::Debug for Notify<'a, T, W> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Notiy").finish()
    }
}

impl<'a, T, W: Wakeup> From<Notify<'a, T, W>> for NotifyHandle {
    fn from(handle: Notify<'a, T, W>) -> NotifyHandle {
        unsafe {
            let ptr = handle.0.clone();
            let ptr = mem::transmute::<Arc<Node<T, W>>, *mut ArcNode<T, W>>(ptr);
            NotifyHandle::new(hide_lt(ptr))
        }
    }
}

struct ArcNode<T, W>(PhantomData<(T, W)>);

// We should never touch `T` on any thread other than the one owning
// `Scheduler`, so this should be a safe operation.
//
// `W` already requires `Sync + Send`
unsafe impl<T, W: Wakeup> Send for ArcNode<T, W> {}
unsafe impl<T, W: Wakeup> Sync for ArcNode<T, W> {}

impl<T, W: Wakeup> executor::Notify for ArcNode<T, W> {
    fn notify(&self, _id: usize) {
        unsafe {
            let me: *const ArcNode<T, W> = self;
            let me: *const *const ArcNode<T, W> = &me;
            let me = me as *const Arc<Node<T, W>>;
            Node::notify(&*me)
        }
    }
}

unsafe impl<T, W: Wakeup> UnsafeNotify for ArcNode<T, W> {
    unsafe fn clone_raw(&self) -> NotifyHandle {
        let me: *const ArcNode<T, W> = self;
        let me: *const *const ArcNode<T, W> = &me;
        let me = &*(me as *const Arc<Node<T, W>>);
        Notify(me).into()
    }

    unsafe fn drop_raw(&self) {
        let mut me: *const ArcNode<T, W> = self;
        let me = &mut me as *mut *const ArcNode<T, W> as *mut Arc<Node<T, W>>;
        ptr::drop_in_place(me);
    }
}

unsafe fn hide_lt<T, W: Wakeup>(p: *mut ArcNode<T, W>) -> *mut UnsafeNotify {
    mem::transmute(p as *mut UnsafeNotify)
}

impl<T, W: Wakeup> Node<T, W> {
    fn notify(me: &Arc<Node<T, W>>) {
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
            inner.enqueue(&**me);
            inner.wakeup.wakeup();
        }
    }
}

impl<T, W> Drop for Node<T, W> {
    fn drop(&mut self) {
        // Currently a `Node<T>` is sent across all threads for any lifetime,
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
    return addr
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
