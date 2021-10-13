use futures::Stream;
use std::{
    collections::VecDeque,
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, ReadBuf};
use tokio_test::{assert_ready, task};
use tokio_util::codec::{BytesCodec, FramedRead};

macro_rules! mock {
    ($($x:expr,)*) => {{
        let mut v = VecDeque::new();
        v.extend(vec![$($x),*]);
        Mock { calls: v }
    }};
}

macro_rules! pin {
    ($id:ident) => {
        Pin::new(&mut $id)
    };
}

macro_rules! assert_read {
    ($e:expr, $n:expr) => {{
        let val = assert_ready!($e);
        assert_eq!(val.unwrap().unwrap(), $n);
    }};
}

#[test]
fn return_none_after_error() {
    let mut io = FramedRead::new(
        mock! {
            Ok(b"abcdef".to_vec()),
            Err(io::Error::new(io::ErrorKind::Other, "Resource errored out")),
        },
        BytesCodec::new(),
    );

    let mut task = task::spawn(());

    task.enter(|cx, _| {
        assert_read!(pin!(io).poll_next(cx), b"abcdef".to_vec());
        let val = assert_ready!(pin!(io).poll_next(cx));
        assert!(val.unwrap().is_err());
        let val = assert_ready!(pin!(io).poll_next(cx));
        assert!(val.is_none());
    })
}

// ================= Mock =================
struct Mock {
    calls: VecDeque<io::Result<Vec<u8>>>,
}

impl AsyncRead for Mock {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        use io::ErrorKind::WouldBlock;

        match self.calls.pop_front() {
            Some(Ok(data)) => {
                debug_assert!(buf.remaining() >= data.len());
                buf.put_slice(&data);
                Poll::Ready(Ok(()))
            }
            Some(Err(ref e)) if e.kind() == WouldBlock => Poll::Pending,
            Some(Err(e)) => Poll::Ready(Err(e)),
            None => Poll::Ready(Ok(())),
        }
    }
}
