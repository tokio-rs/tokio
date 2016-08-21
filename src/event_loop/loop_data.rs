use std::sync::Arc;
use std::io;

use futures::{Future, Poll};
use futures::task;
use futures::executor::Executor;

use event_loop::{Message, Loop, LoopPin, LoopHandle, LoopFuture};
use self::dropbox::DropBox;

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

pub struct Opaque {
    _inner: DropBox<dropbox::MyDrop>,
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

impl Loop {
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
}

impl LoopHandle {

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
            self.handle.send(Message::Drop(Opaque { _inner: data }));
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
    use event_loop::{CURRENT_LOOP, LoopPin};

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

