#![cfg(all(feature = "macros", feature = "rt-multi-thread"))]

#[tokio::main]
async fn basic_main() -> usize {
    1
}

#[tokio::main]
async fn generic_fun<T: Default>() -> T {
    T::default()
}

#[tokio::main]
async fn spawning() -> usize {
    let join = tokio::spawn(async { 1 });
    join.await.unwrap()
}

#[test]
fn main_with_spawn() {
    assert_eq!(1, spawning());
}

#[test]
fn shell() {
    assert_eq!(1, basic_main());
    assert_eq!(bool::default(), generic_fun::<bool>())
}
