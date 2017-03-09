extern crate futures;
#[macro_use]
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_process;
#[macro_use]
extern crate log;
extern crate env_logger;

use std::io;
use std::process::{Stdio, ExitStatus, Command};

use futures::future::{BoxFuture, Future};
use futures::stream::{self, Stream};
use tokio_io::io::{read_until, write_all, read_to_end};
use tokio_core::reactor::Core;
use tokio_process::{CommandExt, Child};

mod support;

fn cat() -> Command {
    let mut cmd = support::cmd("cat");
    cmd.stdin(Stdio::piped())
       .stdout(Stdio::piped());
    cmd
}

fn feed_cat(mut cat: Child, n: usize) -> BoxFuture<ExitStatus, io::Error> {
    let stdin = cat.stdin().take().unwrap();
    let stdout = cat.stdout().take().unwrap();

    debug!("starting to feed");
    // Produce n lines on the child's stdout.
    let numbers = stream::iter((0..n).into_iter().map(Ok));
    let write = numbers.fold(stdin, |stdin, i| {
        debug!("sending line {} to child", i);
        write_all(stdin, format!("line {}\n", i).into_bytes()).map(|p| p.0)
    }).map(|_| ());

    // Try to read `n + 1` lines, ensuring the last one is empty
    // (i.e. EOF is reached after `n` lines.
    let reader = io::BufReader::new(stdout);
    let expected_numbers = stream::iter((0..n + 1).map(Ok));
    let read = expected_numbers.fold((reader, 0), move |(reader, i), _| {
        let done = i >= n;
        debug!("starting read from child");
        read_until(reader, b'\n', Vec::new()).and_then(move |(reader, vec)| {
            debug!("read line {} from child ({} bytes, done: {})",
                   i, vec.len(), done);
            match (done, vec.len()) {
                (false, 0) => {
                    Err(io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe"))
                },
                (true, n) if n != 0 => {
                    Err(io::Error::new(io::ErrorKind::Other, "extraneous data"))
                },
                _ => {
                    let s = std::str::from_utf8(&vec).unwrap();
                    let expected = format!("line {}\n", i);
                    if done || s == expected {
                        Ok((reader, i + 1))
                    } else {
                        Err(io::Error::new(io::ErrorKind::Other, "unexpected data"))
                    }
                }
            }
        })
    });

    // Compose reading and writing concurrently.
    write.join(read).and_then(|_| cat).boxed()
}

#[test]
/// Check for the following properties when feeding stdin and
/// consuming stdout of a cat-like process:
///
/// - A number of lines that amounts to a number of bytes exceeding a
///   typical OS buffer size can be fed to the child without
///   deadlock. This tests that we also consume the stdout
///   concurrently; otherwise this would deadlock.
///
/// - We read the same lines from the child that we fed it.
///
/// - The child does produce EOF on stdout after the last line.
fn feed_a_lot() {
    let mut lp = Core::new().unwrap();
    let child = cat().spawn_async(&lp.handle()).unwrap();
    let status = lp.run(feed_cat(child, 10000)).unwrap();
    assert_eq!(status.code(), Some(0));
}

#[test]
fn drop_kills() {
    let mut lp = Core::new().unwrap();
    let mut child = cat().spawn_async(&lp.handle()).unwrap();
    let stdin = child.stdin().take().unwrap();
    let stdout = child.stdout().take().unwrap();
    drop(child);

    drop(lp.run(write_all(stdin, b"1234")));
    let (_, output) = lp.run(read_to_end(stdout, Vec::new())).unwrap();
    assert_eq!(output.len(), 0);
}

#[test]
fn wait_with_output_captures() {
    let mut core = Core::new().unwrap();

    let mut child = cat().spawn_async(&core.handle()).unwrap();
    let stdin = child.stdin().take().unwrap();
    let out = child.wait_with_output();

    let ret = core.run(write_all(stdin, b"1234").map(|p| p.1).join(out)).unwrap();
    let (written, output) = ret;

    assert!(output.status.success());
    assert_eq!(output.stdout, written);
    assert_eq!(output.stderr.len(), 0);
}
