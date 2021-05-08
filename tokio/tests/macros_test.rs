use tokio::test;

#[test]
async fn test_macro_can_be_used_via_use() {
    tokio::spawn(async {
        assert_eq!(1 + 1, 2);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_macro_is_resilient_to_shadowing() {
    tokio::spawn(async {
        assert_eq!(1 + 1, 2);
    })
    .await
    .unwrap();
}

// https://github.com/tokio-rs/tokio/issues/3403
#[rustfmt::skip] // this `rustfmt::skip` is necessary because unused_braces does not warn if the block contains newline.
#[tokio::main]
pub async fn unused_braces_main() { println!("hello") }
#[rustfmt::skip] // this `rustfmt::skip` is necessary because unused_braces does not warn if the block contains newline.
#[tokio::test]
async fn unused_braces_test() { assert_eq!(1 + 1, 2) }

// https://github.com/tokio-rs/tokio/pull/3766#issuecomment-835508651
#[std::prelude::v1::test]
fn trait_method() {
    trait A {
        fn f(self);
    }
    impl A for () {
        #[tokio::main]
        async fn f(self) {}
    }
    ().f()
}
