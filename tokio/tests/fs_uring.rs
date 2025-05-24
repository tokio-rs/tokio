#![cfg(all(tokio_uring, feature = "rt", feature = "fs", target_os = "linux",))]

use std::sync::mpsc;

use tempfile::NamedTempFile;
use tokio::{
    fs::OpenOptions,
    io::AsyncReadExt,
    runtime::{Builder, Runtime},
    task::JoinSet,
};

fn multi_rt(n: usize) -> Box<dyn Fn() -> Runtime> {
    Box::new(move || {
        Builder::new_multi_thread()
            .worker_threads(n)
            .enable_all()
            .build()
            .unwrap()
    })
}

fn current_rt() -> Box<dyn Fn() -> Runtime> {
    Box::new(|| Builder::new_current_thread().enable_all().build().unwrap())
}

#[test]
fn all_tests() {
    let rt_conbination = vec![current_rt(), multi_rt(1), multi_rt(8)];

    for rt in rt_conbination {
        shutdown_runtime_while_performing_io_uring_ops(rt());
        process_many_files(rt());
    }
}

fn shutdown_runtime_while_performing_io_uring_ops(rt: Runtime) {
    let (tx, rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();

    rt.spawn(async {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        let mut set = JoinSet::new();

        // spawning a bunch of uring operations.
        loop {
            let path = path.clone();
            set.spawn(async move {
                let mut opt = OpenOptions::new();
                opt.read(true);
                opt.open(&path).await.unwrap();
            });
        }
    });

    std::thread::spawn(move || {
        let rt: Runtime = rx.recv().unwrap();
        rt.shutdown_background();
        done_tx.send(()).unwrap();
    });

    tx.send(rt).unwrap();

    done_rx.recv().unwrap();
}

fn process_many_files(rt: Runtime) {
    rt.block_on(async {
        const NUM_FILES: usize = 512;
        const FILE_SIZE: usize = 64;

        use rand::Rng;
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut files = Vec::with_capacity(NUM_FILES);
        for _ in 0..NUM_FILES {
            let mut tmp = NamedTempFile::new().unwrap();
            let mut data = vec![0u8; FILE_SIZE];
            rand::thread_rng().fill(&mut data[..]);
            tmp.write_all(&data).unwrap();
            tmp.flush().unwrap();
            let path = tmp.path().to_path_buf();
            files.push((tmp, data, path));
        }

        let mut handles = Vec::with_capacity(NUM_FILES);
        for (tmp, original, path) in files {
            handles.push(tokio::spawn(async move {
                let _keep_alive = tmp;

                let mut file = tokio::fs::OpenOptions::new()
                    .read(true)
                    .open(&path)
                    .await
                    .unwrap();
                let mut buf = vec![0u8; FILE_SIZE];

                file.read_exact(&mut buf).await.unwrap();

                assert_eq!(buf, original);
            }));
        }

        for h in handles {
            h.await.unwrap();
        }
    });
}
