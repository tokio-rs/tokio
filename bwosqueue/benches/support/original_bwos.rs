#![allow(dead_code)]
use array_init::array_init;
use crossbeam_utils::CachePadded;
use std::cell::UnsafeCell;
use std::cmp::max;
use std::marker::{Send, Sync};
use std::mem::MaybeUninit;
use std::ptr::null_mut;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release, SeqCst};
use std::sync::Arc;

const NB: usize = 8;
const NE: usize = 1024;
const NB_LOG: usize = 3;
const NE_LOG: usize = 11;

#[inline(always)]
fn wsq_global_idx(v: u64) -> u64 {
    return v & ((1 << NB_LOG) - 1);
}

#[inline(always)]
fn wsq_local_idx(v: u64) -> u64 {
    return v & ((1 << NE_LOG) - 1);
}

#[inline(always)]
fn wsq_local_vsn(v: u64) -> u64 {
    return v >> NE_LOG;
}

#[inline(always)]
fn wsq_local_compose(h: u64, l: u64) -> u64 {
    return (h << NE_LOG) | l;
}

#[inline(always)]
fn advance(v: &AtomicU64, old_v: u64) {
    let _ = v.compare_exchange_weak(old_v, old_v + 1, Relaxed, Relaxed);
}

struct BlockConfig<E: 'static> {
    beginning: u8,
    prev: *mut Block<E>,
    next: *mut Block<E>,
}

struct Block<E: 'static> {
    /// producer
    committed: CachePadded<AtomicU64>,
    /// consumer
    consumed: CachePadded<AtomicU64>,
    /// stealer-head
    reserved: CachePadded<AtomicU64>,
    /// stealer-tail
    stealed: CachePadded<AtomicU64>,
    conf: CachePadded<BlockConfig<E>>,
    entries: CachePadded<MaybeUninit<[UnsafeCell<E>; NE]>>,
}

struct BwsQueue<E: 'static> {
    pcache: CachePadded<*mut Block<E>>,
    spos: CachePadded<AtomicU64>,
    ccache: CachePadded<*mut Block<E>>,
    blocks: CachePadded<[UnsafeCell<Block<E>>; NB]>,
}

unsafe impl<E> Send for BwsQueue<E> {}
unsafe impl<E> Sync for BwsQueue<E> {}

impl<E> BlockConfig<E> {
    fn new(idx: usize) -> BlockConfig<E> {
        BlockConfig {
            beginning: if idx == 0 { 1 } else { 0 },
            prev: null_mut(),
            next: null_mut(),
        }
    }
}

impl<E> Block<E> {
    fn new(idx: usize) -> Block<E> {
        let empty_val: u64 = if idx != 0 {
            NE as u64
        } else {
            wsq_local_compose(1, 0)
        };
        let full_val: u64 = if idx != 0 {
            NE as u64
        } else {
            wsq_local_compose(1, NE as u64)
        };
        Block {
            committed: CachePadded::new(AtomicU64::new(empty_val)),
            consumed: CachePadded::new(AtomicU64::new(empty_val)),
            reserved: CachePadded::new(AtomicU64::new(full_val)),
            stealed: CachePadded::new(AtomicU64::new(full_val)),
            conf: CachePadded::new(BlockConfig::<E>::new(idx)),
            entries: CachePadded::new(MaybeUninit::uninit()),
        }
    }

    #[inline(always)]
    fn is_consumed(&mut self, vsn: u64) -> bool {
        let consumed: u64 = self.consumed.load(SeqCst);
        return (wsq_local_idx(consumed) == NE as u64 && wsq_local_vsn(consumed) == vsn)
            || wsq_local_vsn(consumed) > vsn;
    }

    #[inline(always)]
    fn is_stealed(&mut self) -> bool {
        let stealed: u64 = self.stealed.load(SeqCst);
        return wsq_local_idx(stealed) == NE as u64;
    }
}

impl<E> BwsQueue<E> {
    fn new() -> BwsQueue<E> {
        BwsQueue {
            pcache: CachePadded::new(null_mut()),
            spos: CachePadded::new(AtomicU64::new(0)),
            ccache: CachePadded::new(null_mut()),
            blocks: CachePadded::new(array_init(|idx| UnsafeCell::new(Block::new(idx)))),
        }
    }

    #[inline(always)]
    #[allow(dead_code)]
    fn is_empty(&self) -> bool {
        unsafe {
            /* fast check */
            let blk = self.ccache.into_inner();
            let consumed: u64 = (*blk).consumed.load(SeqCst);
            let committed: u64 = (*blk).committed.load(SeqCst);
            return committed == consumed;
        }
    }
}

pub struct Producer<E: 'static> {
    queue: Arc<BwsQueue<E>>,
}

impl<E> Clone for Producer<E> {
    fn clone(&self) -> Self {
        Producer {
            queue: self.queue.clone(),
        }
    }
}

impl<E> Producer<E> {
    #[inline(always)]
    pub fn enqueue(&mut self, t: E) -> bool {
        unsafe {
            loop {
                /* get the address of the alloc block */
                let blk: *mut Block<E> = self.queue.pcache.into_inner();

                /* precheck once */
                let committed: u64 = (*blk).committed.load(Relaxed);
                let committed_idx: u64 = wsq_local_idx(committed);

                /* if out of bound, we don't add the space, but help to move the block */
                if committed_idx < NE as u64 {
                    /* copy the data into the entry and commit it */
                    std::ptr::write(
                        (*(*blk).entries.as_mut_ptr())[committed_idx as usize].get(),
                        t,
                    );
                    (*blk).committed.store(committed + 1, Release);
                    return true;
                }

                /* slow path, all writers help to move to next block */
                let nblk: *mut Block<E> = (*blk).conf.next;
                let next_vsn: u64 = wsq_local_vsn(committed) + (*nblk).conf.beginning as u64;

                /* check if next block is ready */
                if !(*nblk).is_consumed(next_vsn - 1) {
                    return false;
                };
                if !(*nblk).is_stealed() {
                    return false;
                };

                /* reset cursor and advance block */
                let new_cursor: u64 = wsq_local_compose(next_vsn, 0);
                (*nblk).committed.store(new_cursor, Relaxed);
                (*nblk).stealed.store(new_cursor, Relaxed);
                (*nblk).reserved.store(new_cursor, Release);
                let q: *mut BwsQueue<E> = Arc::as_ptr(&self.queue) as *mut _;
                (*q).pcache = CachePadded::new(nblk);
            }
        }
    }
}

pub struct Consumer<E: 'static> {
    queue: Arc<BwsQueue<E>>,
}

impl<E> Clone for Consumer<E> {
    fn clone(&self) -> Self {
        Consumer {
            queue: self.queue.clone(),
        }
    }
}

impl<E> Consumer<E> {
    #[inline(always)]
    pub fn dequeue(&mut self) -> Option<E> {
        unsafe {
            loop {
                /* get the current block */
                let blk: *mut Block<E> = self.queue.ccache.into_inner();

                /* check if the block is fully consumed */
                let consumed: u64 = (*blk).consumed.load(Relaxed);
                let consumed_idx: u64 = wsq_local_idx(consumed);

                if consumed_idx < NE as u64 {
                    /* check if we have an entry to occupy */
                    let committed: u64 = (*blk).committed.load(Relaxed);
                    let committed_idx: u64 = wsq_local_idx(committed);
                    if consumed_idx == committed_idx {
                        return None;
                    }

                    /* we got the entry */
                    let t =
                        std::ptr::read((*(*blk).entries.as_mut_ptr())[consumed_idx as usize].get());
                    (*blk).consumed.store(consumed + 1, Relaxed);
                    return Some(t);
                }

                /* r_head never pass the w_head and r_tail */
                let nblk: *mut Block<E> = (*blk).conf.next;
                let next_cons_vsn: u64 = wsq_local_vsn(consumed) + (*nblk).conf.beginning as u64;
                let next_steal_vsn: u64 = wsq_local_vsn((*nblk).reserved.load(Relaxed));
                if next_steal_vsn != next_cons_vsn {
                    return None;
                }

                /* stop stealers */
                let reserved_new: u64 = wsq_local_compose(next_cons_vsn, NE as u64);
                let reserved_old: u64 = (*nblk).reserved.swap(reserved_new, Relaxed);

                /* pre-steal reserved */
                let reserved_idx: u64 = wsq_local_idx(reserved_old);
                let pre_stealed: u64 = max(0, NE as u64 - reserved_idx);
                (*nblk).stealed.fetch_add(pre_stealed, Relaxed);

                /* advance the block and try again */
                let new_cursor: u64 = wsq_local_compose(next_cons_vsn, reserved_idx);
                (*nblk).consumed.store(new_cursor, Relaxed);
                let q: *mut BwsQueue<E> = Arc::as_ptr(&self.queue) as *mut _;
                (*q).ccache = CachePadded::new(nblk);
            }
        }
    }
}

pub struct Stealer<E: 'static> {
    queue: Arc<BwsQueue<E>>,
}

impl<E> Clone for Stealer<E> {
    fn clone(&self) -> Self {
        Stealer {
            queue: self.queue.clone(),
        }
    }
}

impl<E> Stealer<E> {
    #[inline(always)]
    pub fn steal(&mut self) -> Option<E> {
        unsafe {
            loop {
                /* get the address of the steal block */
                let spos: u64 = self.queue.spos.load(Relaxed);
                let bidx: usize = wsq_global_idx(spos) as usize;
                let blk: *mut Block<E> = self.queue.blocks[bidx].get();

                /* check if the block is fully reserved */
                let reserved: u64 = (*blk).reserved.load(Acquire);
                let reserved_idx: u64 = wsq_local_idx(reserved);

                if reserved_idx < NE as u64 {
                    /* check if we have an entry to occupy */
                    let committed: u64 = (*blk).committed.load(Acquire);
                    let committed_idx: u64 = wsq_local_idx(committed);
                    if reserved_idx == committed_idx {
                        return None;
                    }

                    if !(*blk)
                        .reserved
                        .compare_exchange_weak(reserved, reserved + 1, Release, Relaxed)
                        .is_ok()
                    {
                        return None;
                    }

                    /* we got the entry */
                    let t =
                        std::ptr::read((*(*blk).entries.as_mut_ptr())[reserved_idx as usize].get());
                    (*blk).stealed.fetch_add(1, Release);
                    return Some(t);
                }

                /* r_head never pass the w_head and r_tail */
                let nblk: *mut Block<E> = (*blk).conf.next;
                let next_except_vsn: u64 = wsq_local_vsn(reserved) + (*nblk).conf.beginning as u64;
                let next_actual_vsn: u64 = wsq_local_vsn((*nblk).reserved.load(Relaxed));
                if next_except_vsn != next_actual_vsn {
                    return None;
                }

                /* reset cursor and advance block */
                advance(&self.queue.spos, spos);
            }
        }
    }
}

pub fn new<E: 'static>() -> (Producer<E>, Consumer<E>, Stealer<E>) {
    let qa = Arc::new(BwsQueue::<E>::new());

    let mut blk_start: *mut Block<E> = null_mut();
    let mut blk_pre: *mut Block<E> = null_mut();
    let mut blk: *mut Block<E>;

    for idx in 0..NB {
        blk = qa.blocks[idx].get();
        if blk_start.is_null() {
            blk_start = blk;
        } else {
            unsafe {
                (*blk_pre).conf.next = blk;
                (*blk).conf.prev = blk_pre;
            }
        }
        blk_pre = blk;
        if idx == NB - 1 {
            unsafe {
                (*blk).conf.next = blk_start;
                (*blk_start).conf.prev = blk;
            }
        }
    }
    unsafe {
        let q: *mut BwsQueue<E> = Arc::as_ptr(&qa) as *mut _;
        (*q).pcache = CachePadded::new(blk_start);
        (*q).ccache = CachePadded::new(blk_start);
    }

    let qb = qa.clone();
    let qc = qa.clone();
    (
        Producer { queue: qa },
        Consumer { queue: qb },
        Stealer { queue: qc },
    )
}
