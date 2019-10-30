use tests_build::tokio;

#[tokio::main]
fn main_is_not_async() {}

#[tokio::main(foo)]
async fn main_attr_has_unknown_args() {}

#[tokio::main(threadpool::bar)]
async fn main_attr_has_path_args() {}

#[tokio::test]
fn test_is_not_async() {}

#[tokio::test]
async fn test_fn_has_args(_x: u8) {}

#[tokio::test(foo)]
async fn test_attr_has_args() {}

#[tokio::test]
#[test]
async fn test_has_second_test_attr() {}

fn main() {}
