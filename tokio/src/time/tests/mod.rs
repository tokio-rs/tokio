mod test_delay;

use crate::time::{self, Instant};
use std::time::Duration;

fn assert_send<T: Send>() {}
fn assert_sync<T: Sync>() {}

#[test]
fn registration_is_send_and_sync() {
    use crate::time::delay::Delay;

    assert_send::<Delay>();
    assert_sync::<Delay>();
}

#[test]
#[should_panic]
fn delay_is_eager() {
    let when = Instant::now() + Duration::from_millis(100);
    let _ = time::sleep_until(when);
}
