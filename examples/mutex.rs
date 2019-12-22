//! An example using Tokio's sync Mutex to demonstrate async locking of a resource.
//!
//! There are a few things of note here to pay attention to in this example.
//! 1. The mutex is wrapped in an [`std::sync::Arc`] to allow it to be shared across threads.
//! 2. Each spawned task obtains a lock and releases it on every iteration.
//! 3. Mutation of the data the Mutex is protecting is done by de-referencing the the obtained lock
//!    as seen on lines 23 and 30.
//!
//! Tokio's Mutex works in a simple FIFO (first in, first out) style where as requests for a lock are
//! made Tokio will queue them up and provide a lock when it is that requester's turn. In that way
//! the Mutex is "fair" and predictable in how it distributes the locks to inner data. This is why
//! the output of this program is an in-order count to 50. Locks are released and reacquired
//! after every iteration, so basically, each thread goes to the back of the line after it increments
//! the value once. Also, since there is only a single valid lock at any given time there is no
//! possibility of a race condition when mutating the inner value.
//!
#![warn(rust_2018_idioms)]

use tokio::sync::Mutex;
use std::sync::Arc;


#[tokio::main]
async fn main() {
    let count = Arc::new(Mutex::new(0));

    for _ in 0..5 {
        let my_count = Arc::clone(&count);
        tokio::spawn(async move {
            for _ in 0..10 {
                let mut lock = my_count.lock().await;
                *lock += 1;
                println!("{}", lock);
            }
        });
    }

    loop {
        if *count.lock().await >= 50 {
            break;
        }
    }
    println!("Count hit 50.");
}