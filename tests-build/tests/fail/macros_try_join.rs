use tests_build::tokio;

#[tokio::main]
async fn main() {
    // do not leak `RotatorSelect`
    let _ = tokio::try_join!(async {
        fn foo(_: impl RotatorSelect) {}
    });

    // do not leak `std::task::Poll::Pending`
    let _ = tokio::try_join!(async { Pending });

    // do not leak `std::task::Poll::Ready`
    let _ = tokio::try_join!(async { Ready(0) });

    // do not leak `std::future::Future`
    let _ = tokio::try_join!(async {
        struct MyFuture;

        impl Future for MyFuture {
            type Output = ();

            fn poll(
                self: std::pin::Pin<&mut Self>,
                _cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Self::Output> {
                todo!()
            }
        }
    });

    // do not leak `std::pin::Pin`
    let _ = tokio::try_join!(async {
        let mut x = 5;
        let _ = Pin::new(&mut x);
    });

    // do not leak `std::future::poll_fn`
    let _ = tokio::try_join!(async {
        let _ = poll_fn(|_cx| todo!());
    });
}
