use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use io_uring::squeue;

use crate::platform::linux::uring::driver;

/// In-flight operation
pub(crate) struct Op<T: 'static> {
    // Driver running the operation
    pub(super) driver: Arc<driver::Inner>,

    // Operation index in the slab
    pub(super) index: usize,

    // Per-operation data
    data: Option<T>,
}

/// Operation completion. Returns stored state with the result of the operation.
#[derive(Debug)]
pub(crate) struct Completion<T> {
    pub(crate) data: T,
    pub(crate) result: io::Result<u32>,
    // the field is currently only read in tests
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) flags: u32,
}

unsafe impl Send for Lifecycle {}
unsafe impl Sync for Lifecycle {}
pub(crate) enum Lifecycle {
    /// The operation has been submitted to uring and is currently in-flight
    Submitted,

    /// The submitter is waiting for the completion of the operation
    Waiting(Waker),

    /// The submitter no longer has interest in the operation result. The state
    /// must be passed to the driver and held until the operation completes.
    Ignored(Box<dyn std::any::Any>),

    /// The operation has completed.
    Completed(io::Result<u32>, u32),
}

impl<T> Op<T> {
    /// Create a new operation
    fn new(data: T, inner: &mut driver::InnerIoUring, inner_rc: &Arc<driver::Inner>) -> Op<T> {
        Op {
            driver: inner_rc.clone(),
            index: inner.ops.insert(),
            data: Some(data),
        }
    }

    /// Submit an operation to uring.
    ///
    /// `state` is stored during the operation tracking any state submitted to
    /// the kernel.
    pub(super) fn submit_with<F>(data: T, f: F) -> io::Result<Op<T>>
    where
        F: FnOnce(&mut T) -> squeue::Entry,
    {
        crate::runtime::context::io_uring_handle()
            .unwrap()
            .with(|| {
                driver::CURRENT.with(|inner_rc| {
                    let mut inner_ref = inner_rc.io_uring.lock();
                    let inner = &mut *inner_ref;

                    // If the submission queue is full, flush it to the kernel
                    if inner.uring.submission().is_full() {
                        inner.submit()?;
                    }

                    // Create the operation
                    let mut op = Op::new(data, inner, inner_rc);

                    // Configure the SQE
                    let sqe = f(op.data.as_mut().unwrap()).user_data(op.index as _);

                    {
                        let mut sq = inner.uring.submission();

                        // Push the new operation
                        if unsafe { sq.push(&sqe).is_err() } {
                            unimplemented!("when is this hit?");
                        }
                    }
                    Ok(op)
                })
            })
    }

    /// Try submitting an operation to uring
    pub(super) fn try_submit_with<F>(data: T, f: F) -> io::Result<Op<T>>
    where
        F: FnOnce(&mut T) -> squeue::Entry,
    {
        if driver::CURRENT.is_set() {
            Op::submit_with(data, f)
        } else {
            Err(io::ErrorKind::Other.into())
        }
    }
}

impl<T> Future for Op<T>
where
    T: Unpin + 'static,
{
    type Output = Completion<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use std::mem;

        let me = &mut *self;
        let mut inner = me.driver.io_uring.lock();
        let lifecycle = inner.ops.get_mut(me.index).expect("invalid internal state");

        match mem::replace(lifecycle, Lifecycle::Submitted) {
            Lifecycle::Submitted => {
                *lifecycle = Lifecycle::Waiting(cx.waker().clone());
                Poll::Pending
            }
            Lifecycle::Waiting(waker) if !waker.will_wake(cx.waker()) => {
                *lifecycle = Lifecycle::Waiting(cx.waker().clone());
                Poll::Pending
            }
            Lifecycle::Waiting(waker) => {
                *lifecycle = Lifecycle::Waiting(waker);
                Poll::Pending
            }
            Lifecycle::Ignored(..) => unreachable!(),
            Lifecycle::Completed(result, flags) => {
                inner.ops.remove(me.index);
                me.index = usize::MAX;

                Poll::Ready(Completion {
                    data: me.data.take().expect("unexpected operation state"),
                    result,
                    flags,
                })
            }
        }
    }
}

impl<T> Drop for Op<T> {
    fn drop(&mut self) {
        let mut inner = self.driver.io_uring.lock();
        let lifecycle = match inner.ops.get_mut(self.index) {
            Some(lifecycle) => lifecycle,
            None => return,
        };

        match lifecycle {
            Lifecycle::Submitted | Lifecycle::Waiting(_) => {
                *lifecycle = Lifecycle::Ignored(Box::new(self.data.take()));
            }
            Lifecycle::Completed(..) => {
                inner.ops.remove(self.index);
            }
            Lifecycle::Ignored(..) => unreachable!(),
        }
    }
}

impl Lifecycle {
    pub(super) fn complete(&mut self, result: io::Result<u32>, flags: u32) -> bool {
        use std::mem;

        match mem::replace(self, Lifecycle::Submitted) {
            Lifecycle::Submitted => {
                *self = Lifecycle::Completed(result, flags);
                false
            }
            Lifecycle::Waiting(waker) => {
                *self = Lifecycle::Completed(result, flags);
                waker.wake();
                false
            }
            Lifecycle::Ignored(..) => true,
            Lifecycle::Completed(..) => unreachable!("invalid operation state"),
        }
    }
}

#[cfg(disable)]
mod test {
    use std::sync::Arc;

    use tokio_test::{assert_pending, assert_ready, task};

    use super::*;

    #[test]
    fn op_stays_in_slab_on_drop() {
        let (op, driver, data) = init();
        drop(op);

        assert_eq!(2, Rc::strong_count(&data));

        assert_eq!(1, driver.num_operations());
        release(driver);
    }

    #[test]
    fn poll_op_once() {
        let (op, driver, data) = init();
        let mut op = task::spawn(op);
        assert_pending!(op.poll());
        assert_eq!(2, Rc::strong_count(&data));

        complete(&op, Ok(1));
        assert_eq!(1, driver.num_operations());
        assert_eq!(2, Rc::strong_count(&data));

        assert!(op.is_woken());
        let Completion {
            result,
            flags,
            data: d,
        } = assert_ready!(op.poll());
        assert_eq!(2, Rc::strong_count(&data));
        assert_eq!(1, result.unwrap());
        assert_eq!(0, flags);

        drop(d);
        assert_eq!(1, Rc::strong_count(&data));

        drop(op);
        assert_eq!(0, driver.num_operations());

        release(driver);
    }

    #[test]
    fn poll_op_twice() {
        let (op, driver, ..) = init();
        let mut op = task::spawn(op);
        assert_pending!(op.poll());
        assert_pending!(op.poll());

        complete(&op, Ok(1));

        assert!(op.is_woken());
        let Completion { result, flags, .. } = assert_ready!(op.poll());
        assert_eq!(1, result.unwrap());
        assert_eq!(0, flags);

        release(driver);
    }

    #[test]
    fn poll_change_task() {
        let (op, driver, ..) = init();
        let mut op = task::spawn(op);
        assert_pending!(op.poll());

        let op = op.into_inner();
        let mut op = task::spawn(op);
        assert_pending!(op.poll());

        complete(&op, Ok(1));

        assert!(op.is_woken());
        let Completion { result, flags, .. } = assert_ready!(op.poll());
        assert_eq!(1, result.unwrap());
        assert_eq!(0, flags);

        release(driver);
    }

    #[test]
    fn complete_before_poll() {
        let (op, driver, data) = init();
        let mut op = task::spawn(op);
        complete(&op, Ok(1));
        assert_eq!(1, driver.num_operations());
        assert_eq!(2, Rc::strong_count(&data));

        let Completion { result, flags, .. } = assert_ready!(op.poll());
        assert_eq!(1, result.unwrap());
        assert_eq!(0, flags);

        drop(op);
        assert_eq!(0, driver.num_operations());

        release(driver);
    }

    #[test]
    fn complete_after_drop() {
        let (op, driver, data) = init();
        let index = op.index;
        drop(op);

        assert_eq!(2, Rc::strong_count(&data));

        assert_eq!(1, driver.num_operations());
        driver.inner.borrow_mut().ops.complete(index, Ok(1), 0);
        assert_eq!(1, Rc::strong_count(&data));
        assert_eq!(0, driver.num_operations());
        release(driver);
    }

    fn init() -> (Op<Rc<()>>, crate::driver::Driver, Rc<()>) {
        use self::driver::Driver;

        let driver = Driver::new().unwrap();
        let handle = driver.handle();
        let data = Rc::new(());

        let op = {
            let mut inner = handle.borrow_mut();
            Op::new(data.clone(), &mut inner, &handle)
        };

        (op, driver, data)
    }

    fn complete(op: &Op<Rc<()>>, result: io::Result<u32>) {
        op.driver.borrow_mut().ops.complete(op.index, result, 0);
    }

    fn release(driver: crate::driver::Driver) {
        // Clear ops, we aren't really doing any I/O
        driver.inner.borrow_mut().ops.0.clear();
    }
}
