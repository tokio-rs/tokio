use crate::net::driver::reactor::ScheduledIo;
use crate::util::slab::{Address, Slab};

use loom::sync::Arc;

fn store_val(slab: &Arc<Slab<ScheduledIo>>, readiness: usize) -> Address {
    let key = slab.alloc().expect("allocate slot");

    if let Some(slot) = slab.get(key) {
        slot.set_readiness(key, |_| readiness)
            .expect("generation should still be valid!");
    } else {
        panic!("slab did not contain a value for {:?}", key);
    }

    key
}

fn get_val(slab: &Arc<Slab<ScheduledIo>>, address: Address) -> Option<usize> {
    slab.get(address).and_then(|s| s.get_readiness(address))
}

mod loom_slab;
// mod loom_small_shard;
// mod loom_small_slab;
// mod loom_stack;
