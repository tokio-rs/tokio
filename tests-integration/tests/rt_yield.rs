use tokio::sync::oneshot;
use tokio::task;

async fn spawn_send() {
    let (tx, rx) = oneshot::channel();

    let task = tokio::spawn(async {
        for _ in 0..10 {
            task::yield_now().await;
        }

        tx.send("done").unwrap();
    });

    assert_eq!("done", rx.await.unwrap());
    task.await.unwrap();
}

#[tokio::main(flavor = "current_thread")]
async fn entry_point() {
    spawn_send().await;
}

#[tokio::test]
async fn test_macro() {
    spawn_send().await;
}

#[test]
fn main_macro() {
    entry_point();
}

#[test]
fn manual_rt() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    rt.block_on(async { spawn_send().await });
}
