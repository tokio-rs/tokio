//! A "tiny" example of HTTP request/response handling using transports.
//!
//! This example is intended for *learning purposes* to see how various pieces
//! hook up together and how HTTP can get up and running. Note that this example
//! is written with the restriction that it *can't* use any "big" library other
//! than Tokio, if you'd like a "real world" HTTP library you likely want a
//! crate like Hyper.
//!
//! Code here is based on the `echo-threads` example and implements two paths,
//! the `/plaintext` and `/json` routes to respond with some text and json,
//! respectively. By default this will run I/O on all the cores your system has
//! available, and it doesn't support HTTP request bodies.

#![warn(rust_2018_idioms)]

use bytes::{Buf, BytesMut};
use http::{header::HeaderValue, Request, Response};
#[cfg(not(test))]
use std::{env, error::Error};
use std::{fmt, io};
use tokio_util::codec::{Decoder, Encoder};

#[cfg(not(test))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    use tokio::net::TcpListener;

    // Parse the arguments, bind the TCP socket we'll be listening to, spin up
    // our worker threads, and start shipping sockets to those worker threads.
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());
    let server = TcpListener::bind(&addr).await?;
    println!("Listening on: {addr}");

    loop {
        let (stream, _) = server.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = process(stream).await {
                println!("failed to process connection; error = {e}");
            }
        });
    }
}

#[cfg(not(test))]
async fn process(stream: tokio::net::TcpStream) -> Result<(), Box<dyn Error>> {
    use futures::SinkExt;
    use tokio_stream::StreamExt;
    use tokio_util::codec::Framed;

    let mut transport = Framed::new(stream, Http);

    while let Some(request) = transport.next().await {
        match request {
            Ok(request) => {
                let response = respond(request).await?;
                transport.send(response).await?;
            }
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}

#[cfg(not(test))]
async fn respond(req: Request<()>) -> Result<Response<String>, Box<dyn Error>> {
    use http::StatusCode;
    use serde_derive::Serialize;

    let mut response = Response::builder();
    let body = match req.uri().path() {
        "/plaintext" => {
            response = response.header("Content-Type", "text/plain");
            "Hello, World!".to_string()
        }
        "/json" => {
            response = response.header("Content-Type", "application/json");

            #[derive(Serialize)]
            struct Message {
                message: &'static str,
            }
            serde_json::to_string(&Message {
                message: "Hello, World!",
            })?
        }
        _ => {
            response = response.status(StatusCode::NOT_FOUND);
            String::new()
        }
    };
    let response = response.body(body).map_err(io::Error::other)?;

    Ok(response)
}

struct Http;

/// Implementation of encoding an HTTP response into a `BytesMut`, basically
/// just writing out an HTTP/1.1 response.
impl Encoder<Response<String>> for Http {
    type Error = io::Error;

    fn encode(&mut self, item: Response<String>, dst: &mut BytesMut) -> io::Result<()> {
        use std::fmt::Write;

        write!(
            BytesWrite(dst),
            "\
             HTTP/1.1 {}\r\n\
             Server: Example\r\n\
             Content-Length: {}\r\n\
             Date: {}\r\n\
             ",
            item.status(),
            item.body().len(),
            date::now()
        )
        .unwrap();

        for (k, v) in item.headers() {
            dst.extend_from_slice(k.as_str().as_bytes());
            dst.extend_from_slice(b": ");
            dst.extend_from_slice(v.as_bytes());
            dst.extend_from_slice(b"\r\n");
        }

        dst.extend_from_slice(b"\r\n");
        dst.extend_from_slice(item.body().as_bytes());

        return Ok(());

        // Right now `write!` on `Vec<u8>` goes through io::Write and is not
        // super speedy, so inline a less-crufty implementation here which
        // doesn't go through io::Error.
        struct BytesWrite<'a>(&'a mut BytesMut);

        impl fmt::Write for BytesWrite<'_> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.0.extend_from_slice(s.as_bytes());
                Ok(())
            }

            fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> fmt::Result {
                fmt::write(self, args)
            }
        }
    }
}

/// Implementation of decoding an HTTP request from the bytes we've read so far.
/// This leverages the `httparse` crate to do the actual parsing and then we use
/// that information to construct an instance of a `http::Request` object,
/// trying to avoid allocations where possible.
impl Decoder for Http {
    type Item = Request<()>;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<Request<()>>> {
        const MAX_HEADERS: usize = 1024;

        let mut stack_headers = [httparse::EMPTY_HEADER; 16];
        let mut vec_headers = Vec::new();

        let (amt, req) = loop {
            let header_buf = if vec_headers.is_empty() {
                &mut stack_headers[..]
            } else {
                &mut vec_headers[..]
            };
            let mut r = httparse::Request::new(&mut *header_buf);
            let status = match r.parse(src) {
                Ok(status) => status,
                Err(httparse::Error::TooManyHeaders) => {
                    if header_buf.len() >= MAX_HEADERS {
                        return Err(io::Error::other("too many HTTP headers"));
                    }
                    let new_cap = (header_buf.len() * 2).min(MAX_HEADERS);
                    vec_headers.resize(new_cap, httparse::EMPTY_HEADER);
                    continue;
                }
                Err(e) => {
                    let msg = format!("failed to parse http request: {e:?}");
                    return Err(io::Error::other(msg));
                }
            };

            let amt = match status {
                httparse::Status::Complete(amt) => amt,
                httparse::Status::Partial => return Ok(None),
            };

            if r.version != Some(1) {
                return Err(io::Error::other("only HTTP/1.1 accepted"));
            }

            let method = http::Method::try_from(r.method.unwrap()).map_err(io::Error::other)?;
            let mut ret = Request::builder();
            ret = ret.method(method);
            ret = ret.uri(r.path.unwrap());
            ret = ret.version(http::Version::HTTP_11);

            for header in r.headers {
                let value = HeaderValue::from_bytes(header.value)
                    .map_err(|_| io::Error::other("header decode error"))?;
                ret = ret.header(header.name, value);
            }

            let req = ret.body(()).map_err(io::Error::other)?;
            break (amt, req);
        };

        src.advance(amt);
        Ok(Some(req))
    }
}

mod date {
    use std::cell::RefCell;
    use std::fmt::{self, Write};
    use std::str;
    use std::time::SystemTime;

    use httpdate::HttpDate;

    pub struct Now(());

    /// Returns a struct, which when formatted, renders an appropriate `Date`
    /// header value.
    pub fn now() -> Now {
        Now(())
    }

    // Gee Alex, doesn't this seem like premature optimization. Well you see
    // there Billy, you're absolutely correct! If your server is *bottlenecked*
    // on rendering the `Date` header, well then boy do I have news for you, you
    // don't need this optimization.
    //
    // In all seriousness, though, a simple "hello world" benchmark which just
    // sends back literally "hello world" with standard headers actually is
    // bottlenecked on rendering a date into a byte buffer. Since it was at the
    // top of a profile, and this was done for some competitive benchmarks, this
    // module was written.
    //
    // Just to be clear, though, I was not intending on doing this because it
    // really does seem kinda absurd, but it was done by someone else [1], so I
    // blame them!  :)
    //
    // [1]: https://github.com/rapidoid/rapidoid/blob/f1c55c0555007e986b5d069fe1086e6d09933f7b/rapidoid-commons/src/main/java/org/rapidoid/commons/Dates.java#L48-L66

    struct LastRenderedNow {
        bytes: [u8; 128],
        amt: usize,
        unix_date: u64,
    }

    thread_local!(static LAST: RefCell<LastRenderedNow> = const { RefCell::new(LastRenderedNow {
        bytes: [0; 128],
        amt: 0,
        unix_date: 0,
    }) });

    impl fmt::Display for Now {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            LAST.with(|cache| {
                let mut cache = cache.borrow_mut();
                let now = SystemTime::now();
                let now_unix = now
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map_or(0, |since_epoch| since_epoch.as_secs());
                if cache.unix_date != now_unix {
                    cache.update(now, now_unix);
                }
                f.write_str(cache.buffer())
            })
        }
    }

    impl LastRenderedNow {
        fn buffer(&self) -> &str {
            str::from_utf8(&self.bytes[..self.amt]).unwrap()
        }

        fn update(&mut self, now: SystemTime, now_unix: u64) {
            self.amt = 0;
            self.unix_date = now_unix;
            write!(LocalBuffer(self), "{}", HttpDate::from(now)).unwrap();
        }
    }

    struct LocalBuffer<'a>(&'a mut LastRenderedNow);

    impl fmt::Write for LocalBuffer<'_> {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let start = self.0.amt;
            let end = start + s.len();
            self.0.bytes[start..end].copy_from_slice(s.as_bytes());
            self.0.amt += s.len();
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn append_headers(request: &mut String, count: usize) {
        for i in 0..count {
            request.push_str(&format!("X-Header-{i}: value-{i}\r\n"));
        }
        request.push_str("\r\n");
    }

    #[test]
    fn decode_fewer_than_16_headers() {
        let mut decoder = Http;
        let mut buf = BytesMut::from(
            "GET /plaintext HTTP/1.1\r\n\
             Host: localhost:8080\r\n\
             User-Agent: test\r\n\r\n",
        );

        let req = decoder.decode(&mut buf).unwrap().unwrap();
        assert_eq!(req.method(), http::Method::GET);
        assert_eq!(req.uri(), "/plaintext");
        assert_eq!(req.version(), http::Version::HTTP_11);
        assert_eq!(req.headers().get("host").unwrap(), "localhost:8080");
        assert_eq!(req.headers().get("user-agent").unwrap(), "test");
        assert!(buf.is_empty());
    }

    #[test]
    fn decode_header_counts() {
        let mut decoder = Http;

        for count in [16, 17, 1023, 1024] {
            let mut raw = String::from("GET /json HTTP/1.1\r\n");
            append_headers(&mut raw, count);

            let mut buf = BytesMut::from(raw.as_str());
            let req = decoder.decode(&mut buf).unwrap().unwrap();
            assert_eq!(req.headers().len(), count);
            for i in 0..count {
                let name = format!("x-header-{i}");
                let expected = format!("value-{i}");
                assert_eq!(req.headers().get(&name).unwrap(), expected.as_str());
            }
            assert!(buf.is_empty());
        }
    }

    #[test]
    fn decode_exceeding_max_headers() {
        let mut decoder = Http;
        let mut raw = String::from("GET /plaintext HTTP/1.1\r\n");
        append_headers(&mut raw, 1025);

        let mut buf = BytesMut::from(raw.as_str());
        let err = decoder.decode(&mut buf).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert_eq!(err.to_string(), "too many HTTP headers");
        assert_eq!(buf.as_ref(), raw.as_bytes());
    }

    #[test]
    fn decode_partial_request() {
        let mut decoder = Http;

        for count in [2, 33] {
            let mut raw = String::from("GET /plaintext HTTP/1.1\r\n");
            append_headers(&mut raw, count);

            for split in 0..raw.len() {
                let mut buf = BytesMut::from(&raw.as_bytes()[..split]);
                assert!(decoder.decode(&mut buf).unwrap().is_none());
                assert_eq!(buf.as_ref(), &raw.as_bytes()[..split]);

                buf.extend_from_slice(&raw.as_bytes()[split..]);
                let req = decoder.decode(&mut buf).unwrap().unwrap();
                assert_eq!(req.headers().len(), count);
                assert!(buf.is_empty());
            }
        }
    }

    #[test]
    fn decode_malformed_input() {
        let mut decoder = Http;

        for raw in [
            "GET / HTTP/1.1\r\nBad Header: val\r\n\r\n",
            "GET / HTTP/2.0\r\nHost: example.com\r\n\r\n",
            "GET http:// HTTP/1.1\r\n\r\n",
        ] {
            let mut buf = BytesMut::from(raw);
            let err = decoder.decode(&mut buf).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::Other);
            assert_eq!(buf.as_ref(), raw.as_bytes());
        }
    }

    #[test]
    fn decode_rejects_http_10() {
        let mut decoder = Http;
        let raw = "GET / HTTP/1.0\r\nHost: example.com\r\n\r\n";
        let mut buf = BytesMut::from(raw);

        let err = decoder.decode(&mut buf).unwrap_err();
        assert_eq!(err.to_string(), "only HTTP/1.1 accepted");
        assert_eq!(buf.as_ref(), raw.as_bytes());
    }

    #[test]
    fn decode_pipelined_requests() {
        let mut decoder = Http;

        for count in [0, 33] {
            let mut raw = String::from("GET /plaintext HTTP/1.1\r\nHost: one\r\n");
            append_headers(&mut raw, count);
            let second = "GET /json HTTP/1.1\r\nHost: two\r\n\r\n";
            raw.push_str(second);
            let mut buf = BytesMut::from(raw.as_str());

            let req1 = decoder.decode(&mut buf).unwrap().unwrap();
            assert_eq!(req1.uri(), "/plaintext");
            assert_eq!(req1.headers().get("host").unwrap(), "one");
            assert_eq!(req1.headers().len(), count + 1);
            assert_eq!(buf.as_ref(), second.as_bytes());

            let req2 = decoder.decode(&mut buf).unwrap().unwrap();
            assert_eq!(req2.uri(), "/json");
            assert_eq!(req2.headers().get("host").unwrap(), "two");

            assert!(decoder.decode(&mut buf).unwrap().is_none());
            assert!(buf.is_empty());
        }
    }

    #[test]
    fn decode_duplicate_headers() {
        let mut decoder = Http;
        let mut buf = BytesMut::from(
            "GET /plaintext HTTP/1.1\r\n\
             Accept: text/plain\r\n\
             Accept: text/html\r\n\r\n",
        );

        let req = decoder.decode(&mut buf).unwrap().unwrap();
        let accepts: Vec<_> = req.headers().get_all("accept").iter().collect();
        assert_eq!(accepts.len(), 2);
        assert_eq!(accepts[0], "text/plain");
        assert_eq!(accepts[1], "text/html");
    }
}
