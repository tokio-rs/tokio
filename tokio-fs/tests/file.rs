extern crate futures;
extern crate rand;
extern crate tempdir;
extern crate tokio_fs;
extern crate tokio_io;
extern crate tokio_threadpool;

use tokio_fs::*;
use tokio_io::io;
use tokio_threadpool::*;

use futures::Future;
use futures::future::poll_fn;
use futures::sync::oneshot;
use rand::{thread_rng, Rng};
use tempdir::TempDir;

use std::fs::File as StdFile;
use std::io::Read;

#[test]
fn read_write() {
    const NUM_CHARS: usize = 16 * 1_024;

    let dir = TempDir::new("tokio-fs-tests").unwrap();
    let file_path = dir.path().join("read_write.txt");

    let contents: Vec<u8> = thread_rng().gen_ascii_chars()
        .take(NUM_CHARS)
        .collect::<String>()
        .into();

    let pool = Builder::new()
        .pool_size(1)
        .build();

    let (tx, rx) = oneshot::channel();

    pool.spawn({
        let file_path = file_path.clone();
        let contents = contents.clone();

        File::create(file_path)
            .and_then(|file| file.metadata())
            .inspect(|&(_, ref metadata)| assert!(metadata.is_file()))
            .and_then(move |(file, _)| io::write_all(file, contents))
            .and_then(|(mut file, _)| {
                poll_fn(move || file.poll_sync_all())
            })
            .then(|res| {
                let _ = res.unwrap();
                tx.send(()).unwrap();
                Ok(())
            })
    });

    rx.wait().unwrap();

    let mut file = StdFile::open(&file_path).unwrap();

    let mut dst = vec![];
    file.read_to_end(&mut dst).unwrap();

    assert_eq!(dst, contents);

    pool.spawn({
        File::open(file_path)
            .and_then(|file| io::read_to_end(file, vec![]))
            .then(move |res| {
                let (_, buf) = res.unwrap();
                assert_eq!(buf, contents);
                Ok(())
            })
    });
}
