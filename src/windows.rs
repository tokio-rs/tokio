//! Windows asynchronous process handling.
//!
//! Like with Unix we don't actually have a way of registering a process with an
//! IOCP object. As a result we similarly need another mechanism for getting a
//! signal when a process has exited. For now this is implemented with the
//! `RegisterWaitForSingleObject` function in the kernel32.dll.
//!
//! This strategy is the same that libuv takes and essentially just queues up a
//! wait for the process in a kernel32-specific thread pool. Once the object is
//! notified (e.g. the process exits) then we have a callback that basically
//! just completes a `Oneshot`.
//!
//! The `poll_exit` implementation will attempt to wait for the process in a
//! nonblocking fashion, but failing that it'll fire off a
//! `RegisterWaitForSingleObject` and then wait on the other end of the oneshot
//! from then on out.

extern crate winapi;
extern crate kernel32;
extern crate mio_named_pipes;

use std::io;
use std::os::windows::prelude::*;
use std::os::windows::process::ExitStatusExt;
use std::process::{self, ExitStatus};

use futures::{Future, Poll, Async, Oneshot, Complete, oneshot, Fuse};
use self::mio_named_pipes::NamedPipe;
use tokio_core::reactor::{PollEvented, Handle};

pub struct Child {
    child: process::Child,
    waiting: Option<Waiting>,
}

struct Waiting {
    rx: Fuse<Oneshot<()>>,
    wait_object: winapi::HANDLE,
    tx: *mut Option<Complete<()>>,
}

unsafe impl Sync for Waiting {}
unsafe impl Send for Waiting {}

impl Child {
    pub fn new(child: process::Child, _handle: &Handle) -> Child {
        Child {
            child: child,
            waiting: None,
        }
    }

    pub fn register_stdin(&mut self, handle: &Handle)
                          -> io::Result<Option<ChildStdin>> {
        stdio(self.child.stdin.take(), handle)
    }

    pub fn register_stdout(&mut self, handle: &Handle)
                           -> io::Result<Option<ChildStdout>> {
        stdio(self.child.stdout.take(), handle)
    }

    pub fn register_stderr(&mut self, handle: &Handle)
                           -> io::Result<Option<ChildStderr>> {
        stdio(self.child.stderr.take(), handle)
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    pub fn poll_exit(&mut self) -> Poll<ExitStatus, io::Error> {
        loop {
            if let Some(ref mut w) = self.waiting {
                match w.rx.poll().expect("should not be canceled") {
                    Async::Ready(()) => {}
                    Async::NotReady => return Ok(Async::NotReady),
                }
                let status = try!(try_wait(&self.child)).expect("not ready yet");
                return Ok(status.into())
            }

            if let Some(e) = try!(try_wait(&self.child)) {
                return Ok(e.into())
            }
            let (tx, rx) = oneshot();
            let ptr = Box::into_raw(Box::new(Some(tx)));
            let mut wait_object = 0 as *mut _;
            let rc = unsafe {
                kernel32::RegisterWaitForSingleObject(&mut wait_object,
                                                      self.child.as_raw_handle(),
                                                      Some(callback),
                                                      ptr as *mut _,
                                                      winapi::INFINITE,
                                                      winapi::WT_EXECUTEINWAITTHREAD |
                                                        winapi::WT_EXECUTEONLYONCE)
            };
            if rc == 0 {
                let err = io::Error::last_os_error();
                drop(unsafe { Box::from_raw(ptr) });
                return Err(err)
            }
            self.waiting = Some(Waiting {
                rx: rx.fuse(),
                wait_object: wait_object,
                tx: ptr,
            });
        }
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        unsafe {
            let rc = kernel32::UnregisterWaitEx(self.wait_object,
                                                winapi::INVALID_HANDLE_VALUE);
            if rc == 0 {
                panic!("failed to unregister: {}", io::Error::last_os_error());
            }
            drop(Box::from_raw(self.tx));
        }
    }
}

unsafe extern "system" fn callback(ptr: winapi::PVOID,
                                   _timer_fired: winapi::BOOLEAN) {
    let mut complete = &mut *(ptr as *mut Option<Complete<()>>);
    complete.take().unwrap().complete(());
}

pub fn try_wait(child: &process::Child) -> io::Result<Option<ExitStatus>> {
    unsafe {
        match kernel32::WaitForSingleObject(child.as_raw_handle(), 0) {
            winapi::WAIT_OBJECT_0 => {}
            winapi::WAIT_TIMEOUT => return Ok(None),
            _ => return Err(io::Error::last_os_error()),
        }
        let mut status = 0;
        let rc = kernel32::GetExitCodeProcess(child.as_raw_handle(), &mut status);
        if rc == winapi::FALSE {
            Err(io::Error::last_os_error())
        } else {
            Ok(Some(ExitStatus::from_raw(status)))
        }
    }
}

pub type ChildStdin = PollEvented<NamedPipe>;
pub type ChildStdout = PollEvented<NamedPipe>;
pub type ChildStderr = PollEvented<NamedPipe>;

fn stdio<T>(option: Option<T>, handle: &Handle)
            -> io::Result<Option<PollEvented<NamedPipe>>>
    where T: IntoRawHandle,
{
    let io = match option {
        Some(io) => io,
        None => return Ok(None),
    };
    let pipe = unsafe { NamedPipe::from_raw_handle(io.into_raw_handle()) };
    let io = try!(PollEvented::new(pipe, handle));
    Ok(Some(io))
}
