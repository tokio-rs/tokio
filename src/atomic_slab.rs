use std::cell::UnsafeCell;
use std::marker;
use std::mem;
use std::ops::Deref;
use std::ptr;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::usize;

const N: usize = 1024;
const M: usize = 3;
const SLOT_IDX_MASK: usize = N - 1;

const MAX_STATE: usize = usize::MAX - 3;
const ALLOCATED: usize = usize::MAX - 2;
const REFD: usize = usize::MAX - 1;
const DROP: usize = usize::MAX;

pub struct AtomicSlab<T> {
    next_free: AtomicUsize,
    ever_allocated: AtomicUsize,
    slots: Box<[AtomicUsize; N]>,
    _data: marker::PhantomData<T>,
}

pub struct Ref<'a, T: 'a> {
    idx: usize,
    data: &'a Slot<T>,
    slab: &'a AtomicSlab<T>,
}

struct Slot<T> {
    state: AtomicUsize,
    data: UnsafeCell<T>,
}

impl<T> AtomicSlab<T> {
    pub fn new() -> AtomicSlab<T> {
        AtomicSlab {
            next_free: AtomicUsize::new(MAX_STATE),
            ever_allocated: AtomicUsize::new(0),
            slots: slots(),
            _data: marker::PhantomData,
        }
    }

    pub fn insert(&self, t: T) -> Option<usize> {
        let mut idx = self.next_free.load(SeqCst);
        let slot = loop {
            if idx == MAX_STATE {
                idx = self.ever_allocated.fetch_add(1, SeqCst);
                match self.allocate(&self.slots, idx, 0) {
                    Some(slot) => break slot,
                    None => return None,
                }
            } else {
                // Given our index, drill down to the actual `Slot<T>` so we can
                // take a look at it
                let slot = self.locate(idx).expect("next free points to empty");

                // If this slot is allocated (state >= MAX) then we skip it.
                // Note that if we see this value of `state` then we should be
                // guaranteed that `next_free` has changed due to where we store
                // `REFD` below. This in turn means that we should be able to
                // continue making progress.
                //
                // TODO: is this check necessary? I think we cen rely on the
                //       compare_exchange below to make progress and
                //       exclusivity.
                let state = slot.state.load(SeqCst);
                if state > MAX_STATE {
                    let next = self.next_free.load(SeqCst);
                    assert!(next != idx);
                    idx = next;
                    continue
                }

                // Woohoo we've got a free slot! Try to move our `next_free`
                // poitner to whatever our slot said its next free pointer was.
                // If we succeed at this then we've acquired the slot. If we
                // fail then we try it all again and `next_free` will have
                // changed.
                let res = self.next_free.compare_exchange(idx, state, SeqCst, SeqCst);
                if let Err(next) = res {
                    idx = next;
                    continue
                }
                break slot
            }
        };

        // Once we've got a slot the first thing we do is store our data into
        // it. Only after this do we flag the slot as allocated to ensure that
        // concurrent calls to `get` don't expose uninitialized data.
        //
        // Once the value is stored we flag the slot as allocated and in use
        // (MAX-1) and then we're able to hand it out via `get` and such.
        unsafe {
            ptr::write(slot.data.get(), t);
        }
        let prev = slot.state.swap(ALLOCATED, SeqCst);
        assert!(prev <= MAX_STATE);
        Some(idx)
    }

    /// Fetches the value for a corresponding slab index
    ///
    /// This function will look up `idx` internally, returning the `T`
    /// corresponding to it. If the `idx` is not allocate within this slab then
    /// `None` is returned.
    pub fn get<'a>(&'a self, idx: usize) -> Option<Ref<'a, T>> {
        let slot = match self.locate(idx) {
            Some(slot) => slot,
            None => return None,
        };
        self.lock(idx, slot)
    }

    fn lock<'a>(&'a self, idx: usize, slot: &'a Slot<T>) -> Option<Ref<'a, T>> {
        match slot.state.compare_exchange(ALLOCATED, REFD, SeqCst, SeqCst) {
            Ok(_) => Some(Ref { data: slot, slab: self, idx: idx }),
            Err(_) => None,
        }
    }

    /// Removes an entry from this slab.
    ///
    /// This is unsafe because you must externally ensure that there's no
    /// references handed out from `get` that are active. This is only safe if
    /// no `get` references are alive.
    pub fn flag_remove(&self, idx: usize) {
        let slot = self.locate(idx).expect("index not present");
        match slot.state.swap(DROP, SeqCst) {
            REFD => return, // `Ref` destructor will clean up
            ALLOCATED => {}
            _ => panic!("invalid state to remove a slot"),
        }

        // We should have exclusive access after swapping from ALLOCATED to
        // DROP, so this should be ok to drop the data here. Afterwards we'll
        // flag our `Slot` as available for us (and also as containing no data)
        // just below.
        unsafe {
            self.free(idx, slot);
        }
    }

    pub fn for_each<F>(&self, mut f: F)
        where F: FnMut(&T)
    {
        self._for_each(0, 0, &self.slots, &mut f)
    }

    fn _for_each(&self,
                 idx: usize,
                 level: usize,
                 slots: &[AtomicUsize; N],
                 callback: &mut FnMut(&T)) {
        for (i, slot) in slots.iter().enumerate() {
            let idx = (idx << shift()) | i;
            let val = match slot.load(SeqCst) {
                0 => continue,
                n => n,
            };
            if level < M - 1 {
                let slots = unsafe { &*(val as *const [AtomicUsize; N]) };
                self._for_each(idx, level + 1, slots, callback);
                continue
            }
            let slot = unsafe { &*(val as *const Slot<T>) };
            let slot = match self.lock(idx, slot) {
                Some(r) => r,
                None => continue,
            };
            callback(&slot);
        }
    }

    unsafe fn free(&self, idx: usize, slot: &Slot<T>) {
        drop(ptr::read(slot.data.get()));
        let mut next = self.next_free.load(SeqCst);
        loop {
            assert!(next <= MAX_STATE);
            slot.state.store(next, SeqCst);
            match self.next_free.compare_exchange(next, idx, SeqCst, SeqCst) {
                Ok(_) => return,
                Err(n) => next = n,
            }
        }
    }

    fn allocate(&self,
                arr: &[AtomicUsize; N],
                idx: usize,
                level: usize) -> Option<&Slot<T>> {
        if level == M - 1 {
            let slot = Box::new(Slot {
                state: AtomicUsize::new(MAX_STATE),
                data: UnsafeCell::new(unsafe { mem::uninitialized() }),
            });
            let slot = Box::into_raw(slot);
            let arr_slot = match arr.get(idx) {
                Some(s) => s,
                None => return None,
            };
            match arr_slot.compare_exchange(0, slot as usize, SeqCst, SeqCst) {
                Ok(_) => return unsafe { Some(&*(slot as *const Slot<T>)) },
                Err(_) => panic!("allocated slot previously taken"),
            }
        } else {
            let slot = &arr[idx & SLOT_IDX_MASK];
            let mut next = slot.load(SeqCst);
            if next == 0 {
                let new = slots();
                let new = Box::into_raw(new);
                match slot.compare_exchange(0, new as usize, SeqCst, SeqCst) {
                    Ok(_) => next = new as usize,
                    Err(other) => {
                        next = other;
                        unsafe {
                            drop(Box::from_raw(new));
                        }
                    }
                }
            }
            unsafe {
                self.allocate(&*(next as *const [AtomicUsize; N]),
                              idx >> shift(),
                              level + 1)
            }
        }
    }

    fn locate(&self, mut idx: usize) -> Option<&Slot<T>> {
        let mut arr = &*self.slots;
        let mut level = 0;
        while level < M - 1 {
            let slot = &arr[idx & SLOT_IDX_MASK];
            let next = slot.load(SeqCst);
            if next == 0 {
                return None
            }
            idx >>= shift();
            level += 1;
            unsafe {
                arr = &*(next as *const [AtomicUsize; N]);
            }
        }
        match arr[idx & SLOT_IDX_MASK].load(SeqCst) {
            0 => None,
            n => Some(unsafe { &*(n as *const Slot<T>) })
        }
    }
}

impl<T> Drop for AtomicSlab<T> {
    fn drop(&mut self) {
        free::<T>(&mut self.slots, 0);

        fn free<T>(arr: &mut [AtomicUsize; N], level: usize) {
            for slot in arr.iter_mut() {
                let value = *slot.get_mut();
                if value == 0 {
                    continue
                }

                unsafe {
                    if level == M - 1 {
                        free_slot(value as *mut Slot<T>);
                    } else {
                        let value = value as *mut [AtomicUsize; N];
                        free::<T>(&mut *value, level + 1);
                        drop(Box::from_raw(value));
                    }
                }
            }
        }

        unsafe fn free_slot<T>(slot: *mut Slot<T>) {
            // If this is an allocated slot, drop it directly as we want to drop
            // the data inside.
            let state = *(*slot).state.get_mut();
            assert!(state != REFD);
            assert!(state != DROP);
            if *(*slot).state.get_mut() == ALLOCATED {
                return drop(Box::from_raw(slot))
            }
            assert!(state <= MAX_STATE);

            // If there's no data in here, we need to go through some trickery
            // to avoid dropping uninitialized or duplicate data.
            drop(Vec::from_raw_parts(slot, 0, 1));
        }
    }
}

impl<'a, T> Deref for Ref<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.data.data.get() }
    }
}

impl<'a, T> Drop for Ref<'a, T> {
    fn drop(&mut self) {
        match self.data.state.compare_exchange(REFD, ALLOCATED, SeqCst, SeqCst) {
            Ok(_) => return, // ok, someone else's problem now
            Err(DROP) => { // someone told us to drop this, so we do so
                unsafe { self.slab.free(self.idx, self.data) }
            }
            Err(_) => panic!("invalid state"),
        }
    }
}

fn shift() -> u32 {
    N.trailing_zeros()
}

fn slots() -> Box<[AtomicUsize; N]> {
    let v = (0..N).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>().into_boxed_slice();
    assert_eq!(v.len(), N);
    let ptr: *const AtomicUsize = v.as_ptr();
    unsafe {
        mem::forget(v);
        Box::from_raw(ptr as *mut [AtomicUsize; N])
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::thread;

    use super::*;

    #[test]
    fn smoke() {
        let a = AtomicSlab::new();
        assert_eq!(a.insert(3), Some(0));
        a.flag_remove(0);
    }

    #[test]
    fn smoke2() {
        let a = AtomicSlab::new();
        for i in 0..N * 2 {
            assert_eq!(a.insert(4), Some(i));
        }
        for i in (0..N * 2).rev() {
            a.flag_remove(i);
        }
        for i in 0..N * 2 {
            assert_eq!(a.insert(4), Some(i));
        }
    }

    #[test]
    fn smoke3() {
        let a = AtomicSlab::new();
        for _ in 0..N * 2 {
            assert_eq!(a.insert(5), Some(0));
            a.flag_remove(0);
        }
    }

    #[test]
    fn threads() {
        const N: usize = 4;
        const M: usize = 4000;

        let go = Arc::new(AtomicBool::new(false));
        let a = Arc::new(AtomicSlab::new());
        let threads = (0..N).map(|i| {
            let go = go.clone();
            let a = a.clone();
            thread::spawn(move || {
                while !go.load(SeqCst) {}

                for _ in 0..M {
                    a.insert(i);
                }
            })
        }).collect::<Vec<_>>();
        go.store(true, SeqCst);
        for thread in threads {
            thread.join().unwrap();
        }
        let mut cnts = vec![0; N];
        for i in 0..N * M {
            cnts[*a.get(i).unwrap()] += 1;
            a.flag_remove(i);
        }
        for cnt in cnts {
            assert_eq!(cnt, M);
        }
    }
}
