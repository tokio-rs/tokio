#![cfg(feature = "full")]

tokio::task_local! {
    static REQ_ID: u32;
    pub static FOO: bool;
}

#[tokio::test(flavor = "multi_thread")]
async fn local() {
    let j1 = tokio::spawn(REQ_ID.scope(1, async move {
        assert_eq!(REQ_ID.get(), 1);
        assert_eq!(REQ_ID.get(), 1);
    }));

    let j2 = tokio::spawn(REQ_ID.scope(2, async move {
        REQ_ID.with(|v| {
            assert_eq!(REQ_ID.get(), 2);
            assert_eq!(*v, 2);
        });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert_eq!(REQ_ID.get(), 2);
    }));

    let j3 = tokio::spawn(FOO.scope(true, async move {
        assert!(FOO.get());
    }));

    j1.await.unwrap();
    j2.await.unwrap();
    j3.await.unwrap();
}
