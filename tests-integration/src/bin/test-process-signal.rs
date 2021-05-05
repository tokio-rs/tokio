// https://github.com/tokio-rs/tokio/issues/3550
fn main() {
    for _ in 0..1000 {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        drop(rt);
    }
}
