use tokio::sync::{Condvar, Mutex};

use std::sync::Arc;

#[tokio::test]
async fn it_works() {
    let mutex = Arc::new(Mutex::new(None));
    let condvar = Arc::new(Condvar::new());

    let future = {
        let mutex = mutex.clone();
        let condvar = condvar.clone();

        tokio::spawn(async move {
            let mut lock = mutex.lock().await;

            while lock.is_none() {
                lock = condvar.wait(lock).await;
            }

            lock.unwrap()
        })
    };

    tokio::spawn(async move {
        let mut lock = mutex.lock().await;
        *lock = Some(42);
        condvar.notify_all();
    });

    let result = future.await.unwrap();

    assert_eq!(result, 42);
}
