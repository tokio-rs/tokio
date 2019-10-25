//! A lock-free concurrent slab.

#[cfg(test)]
macro_rules! test_println {
    ($($arg:tt)*) => {
        println!("{:?} {}", crate::net::driver::reactor::dispatch::Tid::current(), format_args!($($arg)*))
    }
}

#[cfg(not(test))]
macro_rules! test_println {
    ($($arg:tt)*) => {};
}

mod iter;
mod pack;
mod page;
mod sharded_slab;
mod tid;

#[cfg(test)]
// this is used by sub-modules
use self::tests::test_util;
use pack::{Pack, WIDTH};
use sharded_slab::Shard;
#[cfg(test)]
pub(crate) use sharded_slab::Slab;
pub(crate) use sharded_slab::{SingleShard, MAX_SOURCES};
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
