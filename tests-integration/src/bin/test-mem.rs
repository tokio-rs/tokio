use futures::future::poll_fn;

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_io()
        .build()
        .unwrap();

    rt.block_on(async {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:0").await.unwrap();
        tokio::spawn(async move {
            loop {
                poll_fn(|cx| listener.poll_accept(cx)).await.unwrap();
            }
        });
    });

    std::thread::sleep(std::time::Duration::from_millis(50));
    drop(rt);
}
