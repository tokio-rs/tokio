//! A lock-free concurrent slab.

#[cfg(test)]
macro_rules! test_println {
    ($($arg:tt)*) => {
        println!("{:?} {}", crate::driver::reactor::sharded_slab::Tid::current(), format_args!($($arg)*))
    }
}

#[cfg(not(test))]
macro_rules! test_println {
    ($($arg:tt)*) => {};
}

mod iter;
mod pack;
mod page;
mod slab;
mod tid;

use super::ScheduledIo;
use pack::{Pack, WIDTH};
use slab::Shard;
pub(crate) use slab::{SingleShard, MAX_SOURCES};
use tid::Tid;

#[cfg(target_pointer_width = "64")]
const MAX_THREADS: usize = 4096;
#[cfg(target_pointer_width = "32")]
const MAX_THREADS: usize = 2048;
const INITIAL_PAGE_SIZE: usize = 32;
const MAX_PAGES: usize = WIDTH / 4;
// Chosen arbitrarily.
const RESERVED_BITS: usize = 5;

#[cfg(test)]
mod tests;
