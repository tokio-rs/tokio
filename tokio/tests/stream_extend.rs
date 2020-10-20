use tokio::stream;

#[tokio::test]
async fn extend_stream() {
    let s = stream::iter(vec![0, 2, 4, 6]);
    let mut buff = vec![-2];
    stream::extend(&mut buff, s).await;
    assert_eq!(buff, vec![-2, 0, 2, 4, 6]);
}
