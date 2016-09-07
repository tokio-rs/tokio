extern crate winapi;
extern crate kernel32;

use std::io;
use std::os::windows::prelude::*;
use std::os::windows::process::ExitStatusExt;
use std::process::{self, ExitStatus};

use futures::{self, Future, Poll, Async, Oneshot, Complete, oneshot, Fuse};

use Command;

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

pub fn spawn(mut cmd: Command) -> Box<Future<Item=Child, Error=io::Error>> {
    Box::new(futures::done(cmd.inner.spawn().map(|c| {
        Child {
            child: c,
            waiting: None,
        }
    })))
}

impl Child {
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }
}

impl Future for Child {
    type Item = ExitStatus;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<ExitStatus, io::Error> {
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
                drop(unsafe { Box::from_raw(ptr) });
                return Err(io::Error::last_os_error())
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
    let mut complete = Box::from_raw(ptr as *mut Option<Complete<()>>);
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
