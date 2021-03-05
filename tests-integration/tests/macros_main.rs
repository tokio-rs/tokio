#![cfg(all(feature = "macros", feature = "rt"))]

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

macro_rules! generate_preserve_none_delimiters_tests {
    ($e:expr) => {
        #[test]
        #[allow(clippy::redundant_closure_call)]
        fn preserve_none_delimiters_in_main() {
            #[tokio::main]
            async fn f() -> i32 {
                $e()
            }

            assert_eq!(f(), ($e)());
        }

        #[tokio::test]
        #[allow(clippy::redundant_closure_call)]
        async fn preserve_none_delimiters_in_test() {
            assert_eq!($e(), ($e)());
        }
    };
}
generate_preserve_none_delimiters_tests!(|| 5);
