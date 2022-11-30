//! Synchronization primitives

cfg_tokio! {
    mod cancellation_token;
    pub use cancellation_token::{
        guard::DropGuard, CancellationToken, WaitForCancellationFuture, WaitForCancellationFutureOwned,
    };
}

cfg_tokio! {
    mod mpsc;
    pub use mpsc::{PollSendError, PollSender};
}

cfg_tokio! {
    mod poll_semaphore;
    pub use poll_semaphore::PollSemaphore;
}

mod reusable_box;
pub use reusable_box::ReusableBoxFuture;
