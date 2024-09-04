#![deny(duplicate_macro_attributes)]

use tests_build::tokio;

#[tokio::main]
fn main_is_not_async() {}

#[tokio::main(foo)]
async fn main_attr_has_unknown_args() {}

#[tokio::main(threadpool::bar)]
async fn main_attr_has_path_args() {}

#[tokio::test]
fn test_is_not_async() {}

#[tokio::test(foo)]
async fn test_attr_has_args() {}

#[tokio::test(foo = 123)]
async fn test_unexpected_attr() {}

#[tokio::test(flavor = 123)]
async fn test_flavor_not_string() {}

#[tokio::test(flavor = "foo")]
async fn test_unknown_flavor() {}

#[tokio::test(flavor = "multi_thread", start_paused = false)]
async fn test_multi_thread_with_start_paused() {}

#[tokio::test(flavor = "multi_thread", worker_threads = "foo")]
async fn test_worker_threads_not_int() {}

#[tokio::test(flavor = "current_thread", worker_threads = 4)]
async fn test_worker_threads_and_current_thread() {}

#[tokio::test(crate = 456)]
async fn test_crate_not_path_int() {}

#[tokio::test(crate = "456")]
async fn test_crate_not_path_invalid() {}

#[tokio::test(flavor = "multi_thread", unhandled_panic = "shutdown_runtime")]
async fn test_multi_thread_with_unhandled_panic() {}

#[tokio::test]
#[test]
async fn test_has_second_test_attr() {}

#[tokio::test]
#[::core::prelude::v1::test]
async fn test_has_second_test_attr_v1() {}

#[tokio::test]
#[core::prelude::rust_2015::test]
async fn test_has_second_test_attr_rust_2015() {}

#[tokio::test]
#[::std::prelude::rust_2018::test]
async fn test_has_second_test_attr_rust_2018() {}

#[tokio::test]
#[std::prelude::rust_2021::test]
async fn test_has_second_test_attr_rust_2021() {}

#[tokio::test]
#[tokio::test]
async fn test_has_generated_second_test_attr() {}

fn main() {}
