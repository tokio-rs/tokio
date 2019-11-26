#![warn(rust_2018_idioms)]

#[tokio::test(timeout = 100)]
#[should_panic(expected = "test `timeout_as_integer_literal` timed out")]
async fn timeout_as_integer_literal() {
    tokio::time::delay_for(std::time::Duration::from_millis(300)).await;
}

#[tokio::test(timeout = "100ms")]
#[should_panic(expected = "test `timeout_as_string_literal` timed out")]
async fn timeout_as_string_literal() {
    tokio::time::delay_for(std::time::Duration::from_millis(300)).await;
}
