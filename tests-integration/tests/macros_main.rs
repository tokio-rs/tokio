#![cfg(feature = "macros")]

#[tokio::main]
async fn basic_main() -> usize {
    1
}

#[cfg(feature = "rt-core")]
mod spawn {
    #[tokio::main]
    async fn spawning() -> usize {
        let join = tokio::spawn(async { 1 });
        join.await.unwrap()
    }

    #[test]
    fn main_with_spawn() {
        assert_eq!(1, spawning());
    }
}

#[test]
fn shell() {
    assert_eq!(1, basic_main());
}
