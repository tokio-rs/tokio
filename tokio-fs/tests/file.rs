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
use std::io::{Read, SeekFrom};

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

    let (tx, rx) = oneshot::channel();

    pool.spawn({
        File::open(file_path)
            .and_then(|file| io::read_to_end(file, vec![]))
            .then(move |res| {
                let (_, buf) = res.unwrap();
                assert_eq!(buf, contents);
                tx.send(()).unwrap();
                Ok(())
            })
    });

    rx.wait().unwrap();
}

#[test]
fn metadata() {
    let dir = TempDir::new("tokio-fs-tests").unwrap();
    let file_path = dir.path().join("metadata.txt");

    let pool = Builder::new().pool_size(1).build();

    let (tx, rx) = oneshot::channel();

    pool.spawn({
        let file_path = file_path.clone();
        let file_path2 = file_path.clone();
        let file_path3 = file_path.clone();

        tokio_fs::metadata(file_path)
            .then(|r| {
                let _ = r.err().unwrap();
                Ok(())
            })
            .and_then(|_| File::create(file_path2))
            .and_then(|_| tokio_fs::metadata(file_path3))
            .then(|r| {
                assert!(r.unwrap().is_file());
                tx.send(())
            })
    });

    rx.wait().unwrap();
}

#[test]
fn seek() {
    let dir = TempDir::new("tokio-fs-tests").unwrap();
    let file_path = dir.path().join("seek.txt");

    let pool = Builder::new().pool_size(1).build();

    let (tx, rx) = oneshot::channel();

    pool.spawn(
        OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(file_path)
            .and_then(|file| io::write_all(file, "Hello, world!"))
            .and_then(|(file, _)| file.seek(SeekFrom::End(-6)))
            .and_then(|(file, _)| io::read_exact(file, vec![0; 5]))
            .and_then(|(file, buf)| {
                assert_eq!(buf, b"world");
                file.seek(SeekFrom::Start(0))
            })
            .and_then(|(file, _)| io::read_exact(file, vec![0; 5]))
            .and_then(|(_, buf)| {
                assert_eq!(buf, b"Hello");
                Ok(())
            })
            .then(|r| {
                let _ = r.unwrap();
                tx.send(())
            }),
    );

    rx.wait().unwrap();
}
