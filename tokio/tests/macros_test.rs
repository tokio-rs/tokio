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
