#![cfg(unix)]

use tokio_stream::StreamExt;

use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio_util::codec::{BytesCodec, FramedRead /*FramedWrite*/};

use bencher::{benchmark_group, benchmark_main, Bencher};

use std::fs::File as StdFile;
use std::io::Read as StdRead;

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .unwrap()
}

const BLOCK_COUNT: usize = 1_000;

const BUFFER_SIZE: usize = 4096;
const DEV_ZERO: &'static str = "/dev/zero";

fn async_read_codec(b: &mut Bencher) {
    let rt = rt();

    b.iter(|| {
        let task = || async {
            let file = File::open(DEV_ZERO).await.unwrap();
            let mut input_stream = FramedRead::with_capacity(file, BytesCodec::new(), BUFFER_SIZE);

            for _i in 0..BLOCK_COUNT {
                let _bytes = input_stream.next().await.unwrap();
            }
        };

        rt.block_on(task());
    });
}

fn async_read_buf(b: &mut Bencher) {
    let rt = rt();

    b.iter(|| {
        let task = || async {
            let mut file = File::open(DEV_ZERO).await.unwrap();
            let mut buffer = [0u8; BUFFER_SIZE];

            for _i in 0..BLOCK_COUNT {
                let count = file.read(&mut buffer).await.unwrap();
                if count == 0 {
                    break;
                }
            }
        };

        rt.block_on(task());
    });
}

fn async_read_std_file(b: &mut Bencher) {
    let rt = rt();

    let task = || async {
        let mut file = tokio::task::block_in_place(|| Box::pin(StdFile::open(DEV_ZERO).unwrap()));

        for _i in 0..BLOCK_COUNT {
            let mut buffer = [0u8; BUFFER_SIZE];
            let mut file_ref = file.as_mut();

            tokio::task::block_in_place(move || {
                file_ref.read_exact(&mut buffer).unwrap();
            });
        }
    };

    b.iter(|| {
        rt.block_on(task());
    });
}

fn sync_read(b: &mut Bencher) {
    b.iter(|| {
        let mut file = StdFile::open(DEV_ZERO).unwrap();
        let mut buffer = [0u8; BUFFER_SIZE];

        for _i in 0..BLOCK_COUNT {
            file.read_exact(&mut buffer).unwrap();
        }
    });
}

benchmark_group!(
    file,
    async_read_std_file,
    async_read_buf,
    async_read_codec,
    sync_read
);

benchmark_main!(file);
