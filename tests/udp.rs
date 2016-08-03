extern crate futures;
extern crate futures_mio;

use futures::Future;
use futures::stream::Stream;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

#[test]
fn send_messages() {
    let mut l = t!(futures_mio::Loop::new());
    let a = l.handle().udp_bind(&"127.0.0.1:0".parse().unwrap());
    let b = l.handle().udp_bind(&"127.0.0.1:0".parse().unwrap());
    let (a, b) = t!(l.run(a.join(b)));
    let a_addr = t!(a.local_addr());
    let b_addr = t!(b.local_addr());

    let ((ar, a), (br, b)) = t!(l.run(a.into_future().join(b.into_future())));
    let ar = ar.unwrap();
    let br = br.unwrap();

    assert!(ar.is_write());
    assert!(!ar.is_read());
    assert!(br.is_write());
    assert!(!br.is_read());

    assert_eq!(t!(a.send_to(b"1234", &b_addr)), 4);
    let (br, b) = t!(l.run(b.into_future()));
    let br = br.unwrap();

    assert!(br.is_read());

    let mut buf = [0; 32];
    let (size, addr) = t!(b.recv_from(&mut buf));
    assert_eq!(size, 4);
    assert_eq!(&buf[..4], b"1234");
    assert_eq!(addr, a_addr);
}
