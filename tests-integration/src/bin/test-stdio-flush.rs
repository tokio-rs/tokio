use tokio::io::{AsyncWriteExt, AsyncReadExt};

fn main() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    rt.block_on(async {
        tokio::io::stdout().write_all(b"Hello world!").await.unwrap();
        tokio::io::stdout().flush().await.unwrap();

        let mut buf = [0];
        tokio::io::stdin().read_exact(&mut buf).await.unwrap();
        assert_eq!(buf[0], 16);
    });
}
