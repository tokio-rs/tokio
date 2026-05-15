#[cfg(feature = "full")]
#[tokio::test]
async fn test_with_semicolon_without_return_type() {
    #![deny(clippy::semicolon_if_nothing_returned)]

    dbg!(0);
}
