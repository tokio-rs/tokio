#![warn(rust_2018_idioms)]

use tokio::runtime::{Builder, Handle};

#[test]
fn has_time_driver_matches_time_feature() {
    let runtime = Builder::new_current_thread().enable_all().build().unwrap();

    assert_eq!(runtime.handle().has_time_driver(), cfg!(feature = "time"));
    runtime.block_on(async {
        assert_eq!(Handle::current().has_time_driver(), cfg!(feature = "time"));
    });
}
