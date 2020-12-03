#[cfg(feature = "unwind")]
#[tokio::test]
#[should_panic]
async fn panic_propagated() {
    tokio::spawn(async {
        panic!();
    });
    tokio::task::yield_now().await;
}

#[cfg(not(feature = "unwind"))]
#[tokio::test]
async fn panic_not_propagated() {
    tokio::spawn(async {
        panic!();
    });
    tokio::task::yield_now().await;
}
