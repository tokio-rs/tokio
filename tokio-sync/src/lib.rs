extern crate futures;

macro_rules! debug {
    ($($t:tt)*) => {
        if false {
            println!($($t)*);
        }
    }
}

mod atomic_task;
mod loom;
mod semaphore;

pub use atomic_task::AtomicTask;
pub use semaphore::{
    Semaphore,
    Waiter as SemaphoreWaiter,
};
