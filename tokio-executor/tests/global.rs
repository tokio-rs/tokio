use tokio_executor::{with_default, DefaultExecutor};

#[test]
fn default_executor_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<DefaultExecutor>();
}

#[test]
#[should_panic]
fn nested_default_executor_status() {
    let _enter = tokio_executor::enter().unwrap();
    let mut executor = DefaultExecutor::current();

    let _result = with_default(&mut executor, || ());
}
