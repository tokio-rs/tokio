#![warn(rust_2018_idioms)]

use tokio_util::context::HandleExt;

use tokio::runtime::Builder;

use tokio::time::*;
#[test]
fn tokio_context_with_another_runtime() {
    let mut rt1 = Builder::new()
        .basic_scheduler()
        .core_threads(1)
        .build()
        .unwrap();
    let rt2 = Builder::new()
        .basic_scheduler()
        .core_threads(1)
        .enable_all()
        .build()
        .unwrap();

    #[allow(unused_attributes)]
    #[should_panic]
    let _ = rt1.block_on(async move {
        delay_for(Duration::from_micros(10)).await;
    });

    let _ = rt1.block_on(async move {
        rt2.handle()
            .clone()
            .wrap(tokio::time::delay_for(Duration::from_micros(10)))
            .await
    });
}
