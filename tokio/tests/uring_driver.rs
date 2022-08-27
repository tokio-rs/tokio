use std::os::unix::prelude::AsRawFd;

use tempfile::NamedTempFile;

use tokio::platform::linux::uring::{buf::IoBuf, fs::File};
use tokio::runtime::Runtime;

#[path = "../src/platform/linux/uring/future.rs"]
#[allow(warnings)]
mod future;

#[test]
fn complete_ops_on_drop() {
    use std::sync::Arc;

    struct MyBuf {
        data: Vec<u8>,
        _ref_cnt: Arc<()>,
    }

    unsafe impl IoBuf for MyBuf {
        fn stable_ptr(&self) -> *const u8 {
            self.data.stable_ptr()
        }

        fn bytes_init(&self) -> usize {
            self.data.bytes_init()
        }

        fn bytes_total(&self) -> usize {
            self.data.bytes_total()
        }
    }

    unsafe impl tokio::platform::linux::uring::buf::IoBufMut for MyBuf {
        fn stable_mut_ptr(&mut self) -> *mut u8 {
            self.data.stable_mut_ptr()
        }

        unsafe fn set_init(&mut self, pos: usize) {
            self.data.set_init(pos);
        }
    }

    // Used to test if the buffer dropped.
    let ref_cnt = Arc::new(());

    let tempfile = tempfile();

    let vec = vec![0; 50 * 1024 * 1024];
    let mut file = std::fs::File::create(tempfile.path()).unwrap();
    std::io::Write::write_all(&mut file, &vec).unwrap();

    let file = Runtime::new().unwrap().block_on(async {
        let file = File::create(tempfile.path()).await.unwrap();
        poll_once(async {
            file.read_at(
                MyBuf {
                    data: vec![0; 64 * 1024],
                    _ref_cnt: ref_cnt.clone(),
                },
                25 * 1024 * 1024,
            )
            .await
            .0
            .unwrap();
        })
        .await;

        file
    });

    assert_eq!(Arc::strong_count(&ref_cnt), 1);

    // little sleep
    std::thread::sleep(std::time::Duration::from_millis(100));

    drop(file);
}

#[tokio::test]
async fn too_many_submissions() {
    let tempfile = tempfile();

    let file = File::create(tempfile.path()).await.unwrap();
    for _ in 0..600 {
        poll_once(async {
            file.write_at(b"hello world".to_vec(), 0).await.0.unwrap();
        })
        .await;
    }
}

fn tempfile() -> NamedTempFile {
    NamedTempFile::new().unwrap()
}

async fn poll_once(future: impl std::future::Future) {
    // use std::future::Future;
    use std::task::Poll;
    use tokio::pin;

    pin!(future);

    future::poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}
