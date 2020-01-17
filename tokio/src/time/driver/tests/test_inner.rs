#![warn(rust_2018_idioms)]

use crate::park::Unpark;
use crate::time::driver::Inner;
use crate::time::Instant;

struct MockUnpark;

impl Unpark for MockUnpark {
    fn unpark(&self) {}
}

#[test]
fn sanity() {
    let unpark = Box::new(MockUnpark);
    let instant = Instant::now();

    let inner = Inner::new(instant, unpark);

    assert_eq!(inner.elapsed(), 0);
}
