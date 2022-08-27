use tokio::platform::linux::uring::buf::{IoBuf, IoBufMut};

use std::mem;

#[test]
fn test_vec() {
    let mut v = vec![];

    assert_eq!(v.as_ptr(), v.stable_ptr());
    assert_eq!(v.as_mut_ptr(), v.stable_mut_ptr());
    assert_eq!(v.bytes_init(), 0);
    assert_eq!(v.bytes_total(), 0);

    v.reserve(100);

    assert_eq!(v.as_ptr(), v.stable_ptr());
    assert_eq!(v.as_mut_ptr(), v.stable_mut_ptr());
    assert_eq!(v.bytes_init(), 0);
    assert_eq!(v.bytes_total(), v.capacity());

    v.extend(b"hello");

    assert_eq!(v.as_ptr(), v.stable_ptr());
    assert_eq!(v.as_mut_ptr(), v.stable_mut_ptr());
    assert_eq!(v.bytes_init(), 5);
    assert_eq!(v.bytes_total(), v.capacity());

    // Assume init does not go backwards
    unsafe {
        v.set_init(3);
    }
    assert_eq!(&v[..], b"hello");

    // Initializing goes forward
    unsafe {
        std::ptr::copy(DATA.as_ptr(), v.stable_mut_ptr(), 10);
        v.set_init(10);
    }

    assert_eq!(&v[..], &DATA[..10]);
}

#[test]
fn test_slice() {
    let v = &b""[..];

    assert_eq!(v.as_ptr(), v.stable_ptr());
    assert_eq!(v.bytes_init(), 0);
    assert_eq!(v.bytes_total(), 0);

    let v = &b"hello"[..];

    assert_eq!(v.as_ptr(), v.stable_ptr());
    assert_eq!(v.bytes_init(), 5);
    assert_eq!(v.bytes_total(), 5);
}

const DATA: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789!?";

macro_rules! test_slice {
    (
        $( $name:ident => $buf:expr; )*
    ) => {
        $(
            mod $name {
                use super::*;

                #[test]
                fn test_slice_read() {
                    let buf = $buf;

                    let slice = buf.slice(..);
                    assert_eq!(slice.begin(), 0);
                    assert_eq!(slice.end(), DATA.len());

                    assert_eq!(&slice[..], DATA);
                    assert_eq!(&slice[5..], &DATA[5..]);
                    assert_eq!(&slice[10..15], &DATA[10..15]);
                    assert_eq!(&slice[..15], &DATA[..15]);

                    let buf = slice.into_inner();

                    let slice = buf.slice(10..);
                    assert_eq!(slice.begin(), 10);
                    assert_eq!(slice.end(), DATA.len());

                    assert_eq!(&slice[..], &DATA[10..]);
                    assert_eq!(&slice[10..], &DATA[20..]);
                    assert_eq!(&slice[5..15], &DATA[15..25]);
                    assert_eq!(&slice[..15], &DATA[10..25]);

                    let buf = slice.into_inner();

                    let slice = buf.slice(5..15);
                    assert_eq!(slice.begin(), 5);
                    assert_eq!(slice.end(), 15);

                    assert_eq!(&slice[..], &DATA[5..15]);
                    assert_eq!(&slice[5..], &DATA[10..15]);
                    assert_eq!(&slice[5..8], &DATA[10..13]);
                    assert_eq!(&slice[..5], &DATA[5..10]);
                    let buf = slice.into_inner();

                    let slice = buf.slice(..15);
                    assert_eq!(slice.begin(), 0);
                    assert_eq!(slice.end(), 15);

                    assert_eq!(&slice[..], &DATA[..15]);
                    assert_eq!(&slice[5..], &DATA[5..15]);
                    assert_eq!(&slice[5..10], &DATA[5..10]);
                    assert_eq!(&slice[..5], &DATA[..5]);
                }
            }
        )*
    };
}

test_slice! {
    vec => Vec::from(DATA);
    slice => DATA;
}

#[test]
fn can_deref_slice_into_uninit_buf() {
    let buf = Vec::with_capacity(10).slice(..);
    let _ = buf.stable_ptr();
    assert_eq!(buf.bytes_init(), 0);
    assert_eq!(buf.bytes_total(), 10);
    assert!(buf[..].is_empty());

    let mut v = Vec::with_capacity(10);
    v.push(42);
    let mut buf = v.slice(..);
    let _ = buf.stable_mut_ptr();
    assert_eq!(buf.bytes_init(), 1);
    assert_eq!(buf.bytes_total(), 10);
    assert_eq!(mem::replace(&mut buf[0], 0), 42);
    buf.copy_from_slice(&[43]);
    assert_eq!(&buf[..], &[43]);
}
