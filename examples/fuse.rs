// This "test" is based on the code sample in #3442
// It is meant to test that `readable()` will surface the
// polling error to the caller.
// This program has to be run as `root` to be able to use
// fuse.
// The expected output is a polling error along the lines of:
// ```
// thread 'main' panicked at 'called `Result::unwrap()` on an `Err` value: Custom { kind: Other, error: "Polling error" }', examples/fuse.rs:44:14
// ```
// This is just to show what testing I'm doing, and I don't intend
// to get it merged
use tokio::io::{unix::AsyncFd, Interest};

use nix::{
    fcntl::{fcntl, open, FcntlArg, OFlag},
    mount::{mount, umount, MsFlags},
    sys::stat::Mode,
    unistd::read,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let fuse = open("/dev/fuse", OFlag::O_RDWR, Mode::empty()).unwrap();
    let fcntl_arg = FcntlArg::F_SETFL(OFlag::O_NONBLOCK | OFlag::O_LARGEFILE);
    fcntl(fuse, fcntl_arg).unwrap();

    let options = format!(
        "fd={},user_id=0,group_id=0,allow_other,rootmode=40000",
        fuse
    );

    mount(
        Some("test"),
        "/mnt",
        Some("fuse"),
        MsFlags::empty(),
        Some(options.as_str()),
    )
    .unwrap();

    let async_fd = AsyncFd::with_interest(fuse, Interest::READABLE).unwrap();

    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(2));
        umount("/mnt").unwrap();
    });

    let mut buffer = [0; 8192];
    loop {
        let result = async_fd
            .readable()
            .await
            .unwrap()
            .try_io(|_| read(fuse, &mut buffer).map_err(Into::into));

        match result {
            Err(_) => continue,
            Ok(result) => {
                result.unwrap();
            }
        }
    }
}
