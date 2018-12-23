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
pub mod mpsc;
mod semaphore;

pub use atomic_task::AtomicTask;
pub use semaphore::{
    Permit,
    Semaphore,
};
