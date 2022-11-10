use std::num::NonZeroU64;

#[derive(Eq, PartialEq, Clone, Copy, Hash, Debug)]
pub(crate) struct ThreadId(NonZeroU64);

impl ThreadId {
    cfg_has_atomic_u64! {
        pub(crate) fn new() -> ThreadId {
            use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

            static COUNTER: AtomicU64 = AtomicU64::new(0);

            let mut last = COUNTER.load(Relaxed);
            loop {
                let id = match last.checked_add(1) {
                    Some(id) => id,
                    None => exhausted(),
                };

                match COUNTER.compare_exchange_weak(last, id, Relaxed, Relaxed) {
                    Ok(_) => return ThreadId(NonZeroU64::new(id).unwrap()),
                    Err(id) => last = id,
                }
            }
        }
    }

    cfg_not_has_atomic_u64! {
        cfg_has_const_mutex_new! {
            pub(crate) fn new() -> ThreadId {
                todo!();
            }
        }

        cfg_not_has_const_mutex_new! {
            pub(crate) fn new() -> ThreadId {
                todo!();
            }
        }
    }
}

#[cold]
fn exhausted() -> ! {
    panic!("failed to generate unique thread ID: bitspace exhausted")
}
