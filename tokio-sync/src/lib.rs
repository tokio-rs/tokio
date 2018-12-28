extern crate futures;

macro_rules! debug {
    ($($t:tt)*) => {
        if false {
            println!($($t)*);
        }
    }
}

macro_rules! if_fuzz {
    ($($t:tt)*) => {{
        if false { $($t)* }
    }}
}

mod atomic_task;
mod loom;
// pub mod oneshot;
pub mod mpsc;
mod semaphore;

pub use atomic_task::AtomicTask;
pub use semaphore::{
    Permit,
    Semaphore,
};
