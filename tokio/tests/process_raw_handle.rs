#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]
#![cfg(windows)]
#![cfg(not(miri))]

use std::future::Future;
use std::os::windows::io::RawHandle;
use std::pin::pin;
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::task::{Context, Wake, Waker};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use windows_sys::Win32::System::Threading::GetProcessId;

#[tokio::test]
async fn obtain_raw_handle() {
    let mut cmd = Command::new("cmd");
    cmd.kill_on_drop(true);
    cmd.arg("/c");
    cmd.arg("pause");

    let child = cmd.spawn().unwrap();

    let orig_id = child.id().expect("missing id");
    assert!(orig_id > 0);

    let handle = child.raw_handle().expect("process stopped");
    let handled_id = unsafe { GetProcessId(handle as _) };
    assert_eq!(handled_id, orig_id);
}

#[tokio::test]
async fn drop_pending_child_unregisters_wait_before_closing_handle() {
    struct CheckHandleOnDrop {
        handle: RawHandle,
        observed_id: Arc<AtomicU32>,
    }

    // SAFETY: `RawHandle` is only inspected via `GetProcessId` when the waker drops.
    unsafe impl Send for CheckHandleOnDrop {}
    unsafe impl Sync for CheckHandleOnDrop {}

    #[allow(unknown_lints, clippy::manual_noop_waker)]
    impl Wake for CheckHandleOnDrop {
        fn wake(self: Arc<Self>) {}
    }

    impl Drop for CheckHandleOnDrop {
        fn drop(&mut self) {
            let id = unsafe { GetProcessId(self.handle as _) };
            self.observed_id.store(id, Ordering::SeqCst);
        }
    }

    let mut cmd = Command::new("cmd");
    cmd.arg("/c");
    cmd.arg("pause");
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::null());

    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();

    let orig_id = child.id().expect("missing id");
    assert!(orig_id > 0);
    let handle = child.raw_handle().expect("missing handle");

    let observed_id = Arc::new(AtomicU32::new(0));
    let waker = Waker::from(Arc::new(CheckHandleOnDrop {
        handle,
        observed_id: observed_id.clone(),
    }));

    {
        let mut wait = pin!(child.wait());
        let mut cx = Context::from_waker(&waker);
        assert!(wait.as_mut().poll(&mut cx).is_pending());
    }
    drop(waker);
    assert_eq!(observed_id.load(Ordering::SeqCst), 0);

    // Dropping `child` must unregister and drop `Waiting` (and its stored waker)
    // before `StdChild` closes the process handle.
    drop(child);
    assert_eq!(observed_id.load(Ordering::SeqCst), orig_id);

    let _ = stdin.write_all(b"\r\n").await;
    drop(stdin);
}
