extern crate futures;
extern crate rand;
extern crate tempfile;
extern crate tokio_fs;
extern crate tokio_io;

use tokio_fs::*;
use tokio_io::io;

use futures::future::poll_fn;
use futures::Future;
use rand::{distributions, thread_rng, Rng};
use tempfile::Builder as TmpBuilder;

use std::fs;
use std::io::SeekFrom;

mod pool;

#[test]
fn read_write() {
    const NUM_CHARS: usize = 16 * 1_024;

    let dir = TmpBuilder::new()
        .prefix("tokio-fs-tests")
        .tempdir()
        .unwrap();
    let file_path = dir.path().join("read_write.txt");

    let contents: Vec<u8> = thread_rng()
        .sample_iter(&distributions::Alphanumeric)
        .take(NUM_CHARS)
        .collect::<String>()
        .into();

    pool::run({
        let file_path = file_path.clone();
        let contents = contents.clone();

        File::create(file_path)
            .and_then(|file| file.metadata())
            .inspect(|&(_, ref metadata)| assert!(metadata.is_file()))
            .and_then(move |(file, _)| io::write_all(file, contents))
            .and_then(|(mut file, _)| poll_fn(move || file.poll_sync_all()))
            .then(|res| {
                let _ = res.unwrap();
                Ok(())
            })
    });

    let dst = fs::read(&file_path).unwrap();
    assert_eq!(dst, contents);

    pool::run({
        File::open(file_path)
            .and_then(|file| io::read_to_end(file, vec![]))
            .then(move |res| {
                let (_, buf) = res.unwrap();
                assert_eq!(buf, contents);
                Ok(())
            })
    });
}

#[test]
fn read_write_helpers() {
    const NUM_CHARS: usize = 16 * 1_024;

    let dir = TmpBuilder::new()
        .prefix("tokio-fs-tests")
        .tempdir()
        .unwrap();
    let file_path = dir.path().join("read_write_all.txt");

    let contents: Vec<u8> = thread_rng()
        .sample_iter(&distributions::Alphanumeric)
        .take(NUM_CHARS)
        .collect::<String>()
        .into();

    pool::run(write(file_path.clone(), contents.clone()).then(|res| {
        let _ = res.unwrap();
        Ok(())
    }));

    let dst = fs::read(&file_path).unwrap();
    assert_eq!(dst, contents);

    pool::run({
        read(file_path).then(move |res| {
            let buf = res.unwrap();
            assert_eq!(buf, contents);
            Ok(())
        })
    });
}

#[test]
fn metadata() {
    let dir = TmpBuilder::new()
        .prefix("tokio-fs-tests")
        .tempdir()
        .unwrap();
    let file_path = dir.path().join("metadata.txt");

    pool::run({
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
                Ok(())
            })
    });
}

#[test]
fn seek() {
    let dir = TmpBuilder::new()
        .prefix("tokio-fs-tests")
        .tempdir()
        .unwrap();
    let file_path = dir.path().join("seek.txt");

    pool::run({
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
                Ok(())
            })
    });
}

#[test]
fn clone() {
    let dir = TmpBuilder::new()
        .prefix("tokio-fs-tests")
        .tempdir()
        .unwrap();
    let file_path = dir.path().join("clone.txt");

    pool::run(
        File::create(file_path.clone())
            .and_then(|file| {
                file.try_clone()
                    .map_err(|(_file, err)| err)
                    .and_then(|(file, clone)| {
                        io::write_all(file, "clone ")
                            .and_then(|_| io::write_all(clone, "successful"))
                    })
            })
            .then(|res| {
                let _ = res.unwrap();
                Ok(())
            }),
    );

    let mut file = StdFile::open(&file_path).unwrap();

    let mut dst = vec![];
    file.read_to_end(&mut dst).unwrap();

    assert_eq!(dst, b"clone successful")
}
