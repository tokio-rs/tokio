#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::fs;

#[tokio::test]
async fn copy() {
    fs::write("foo.txt", b"Hello File!").await.unwrap();
    fs::copy("foo.txt", "bar.txt").await.unwrap();

    let from = fs::read("foo.txt").await.unwrap();
    let to = fs::read("bar.txt").await.unwrap();

    assert_eq!(from, to);
}
