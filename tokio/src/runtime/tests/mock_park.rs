#![allow(warnings)]

use crate::runtime::{Park, Unpark};

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::Arc;
use std::time::Duration;

pub struct MockPark {
    parks: HashMap<usize, Arc<Inner>>,
}

#[derive(Clone)]
struct ParkImpl(Arc<Inner>);

struct Inner {
    unparked: AtomicBool,
}

impl MockPark {
    pub fn new() -> MockPark {
        MockPark {
            parks: HashMap::new(),
        }
    }

    pub fn is_unparked(&self, index: usize) -> bool {
        self.parks[&index].unparked.load(SeqCst)
    }

    pub fn clear(&self, index: usize) {
        self.parks[&index].unparked.store(false, SeqCst);
    }

    pub fn mk_park(&mut self, index: usize) -> impl Park {
        let inner = Arc::new(Inner {
            unparked: AtomicBool::new(false),
        });
        self.parks.insert(index, inner.clone());
        ParkImpl(inner)
    }
}

impl Park for ParkImpl {
    type Unpark = ParkImpl;
    type Error = ();

    fn unpark(&self) -> Self::Unpark {
        self.clone()
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        unimplemented!();
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        unimplemented!();
    }
}

impl Unpark for ParkImpl {
    fn unpark(&self) {
        self.0.unparked.store(true, SeqCst);
    }
}
