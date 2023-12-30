//! Utility to help with "really nice to add a warning for tasks that might be blocking"
mod lrtd;

pub use self::lrtd::BlockingActionHandler;
pub use self::lrtd::LongRunningTaskDetector;
pub use self::lrtd::ThreadStateHandler;
