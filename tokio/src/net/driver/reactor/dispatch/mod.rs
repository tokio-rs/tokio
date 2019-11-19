//! A lock-free concurrent slab.

mod pack;
use pack::{Pack, WIDTH};

mod page;
pub(crate) use page::ScheduledIo;

mod sharded_slab;
pub(crate) use sharded_slab::{Slab, MAX_SOURCES};

mod tid;
use tid::Tid;

// this is used by sub-modules
#[cfg(all(test, loom))]
use self::tests::test_util;

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
