//! A "tiny" example of HTTP request/response handling using just tokio-core
//!
//! This example is intended for *learning purposes* to see how various pieces
//! hook up together and how HTTP can get up and running. Note that this example
//! is written with the restriction that it *can't* use any "big" library other
//! than tokio-core, if you'd like a "real world" HTTP library you likely want a
//! crate like Hyper.
//!
//! Code here is based on the `echo-threads` example and implements two paths,
//! the `/plaintext` and `/json` routes to respond with some text and json,
//! respectively. By default this will run I/O on all the cores your system has
//! available, and it doesn't support HTTP request bodies.

#![deny(warnings)]

extern crate bytes;
extern crate http;
extern crate httparse;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate time;
extern crate tokio;
extern crate tokio_io;

use std::{env, fmt, io};
use std::net::SocketAddr;

use tokio::net::{TcpStream, TcpListener};
use tokio::prelude::*;
use tokio::codec::{Encoder, Decoder};

use bytes::BytesMut;
use http::header::HeaderValue;
use http::{Request, Response, StatusCode};

fn main() {
    // Parse the arguments, bind the TCP socket we'll be listening to, spin up
    // our worker threads, and start shipping sockets to those worker threads.
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    let listener = TcpListener::bind(&addr).expect("failed to bind");
    println!("Listening on: {}", addr);

    tokio::run({
        listener.incoming()
            .map_err(|e| println!("failed to accept socket; error = {:?}", e))
            .for_each(|socket| {
                process(socket);
                Ok(())
            })
    });
}

fn process(socket: TcpStream) {
    let (tx, rx) =
        // Frame the socket using the `Http` protocol. This maps the TCP socket
        // to a Stream + Sink of HTTP frames.
        Http.framed(socket)
        // This splits a single `Stream + Sink` value into two separate handles
        // that can be used independently (even on different tasks or threads).
        .split();

    // Map all requests into responses and send them back to the client.
    let task = tx.send_all(rx.and_then(respond))
        .then(|res| {
            if let Err(e) = res {
                println!("failed to process connection; error = {:?}", e);
            }

            Ok(())
        });

    // Spawn the task that handles the connection.
    tokio::spawn(task);
}

/// "Server logic" is implemented in this function.
///
/// This function is a map from and HTTP request to a future of a response and
/// represents the various handling a server might do. Currently the contents
/// here are pretty uninteresting.
fn respond(req: Request<()>)
    -> Box<Future<Item = Response<String>, Error = io::Error> + Send>
{
    let mut ret = Response::builder();
    let body = match req.uri().path() {
        "/plaintext" => {
            ret.header("Content-Type", "text/plain");
            "Hello, World!".to_string()
        }
        "/json" => {
            ret.header("Content-Type", "application/json");

            #[derive(Serialize)]
            struct Message {
                message: &'static str,
            }
            serde_json::to_string(&Message { message: "Hello, World!" })
                .unwrap()
        }
        _ => {
            ret.status(StatusCode::NOT_FOUND);
            String::new()
        }
    };
    Box::new(future::ok(ret.body(body).unwrap()))
}

struct Http;

/// Implementation of encoding an HTTP response into a `BytesMut`, basically
/// just writing out an HTTP/1.1 response.
impl Encoder for Http {
    type Item = Response<String>;
    type Error = io::Error;

    fn encode(&mut self, item: Response<String>, dst: &mut BytesMut) -> io::Result<()> {
        use std::fmt::Write;

        write!(BytesWrite(dst), "\
            HTTP/1.1 {}\r\n\
            Server: Example\r\n\
            Content-Length: {}\r\n\
            Date: {}\r\n\
        ", item.status(), item.body().len(), date::now()).unwrap();

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

        impl<'a> fmt::Write for BytesWrite<'a> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.0.extend_from_slice(s.as_bytes());
                Ok(())
            }

            fn write_fmt(&mut self, args: fmt::Arguments) -> fmt::Result {
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
        // TODO: we should grow this headers array if parsing fails and asks
        //       for more headers
        let mut headers = [None; 16];
        let (method, path, version, amt) = {
            let mut parsed_headers = [httparse::EMPTY_HEADER; 16];
            let mut r = httparse::Request::new(&mut parsed_headers);
            let status = r.parse(src).map_err(|e| {
                let msg = format!("failed to parse http request: {:?}", e);
                io::Error::new(io::ErrorKind::Other, msg)
            })?;

            let amt = match status {
                httparse::Status::Complete(amt) => amt,
                httparse::Status::Partial => return Ok(None),
            };

            let toslice = |a: &[u8]| {
                let start = a.as_ptr() as usize - src.as_ptr() as usize;
                assert!(start < src.len());
                (start, start + a.len())
            };

            for (i, header) in r.headers.iter().enumerate() {
                let k = toslice(header.name.as_bytes());
                let v = toslice(header.value);
                headers[i] = Some((k, v));
            }

            (toslice(r.method.unwrap().as_bytes()),
             toslice(r.path.unwrap().as_bytes()),
             r.version.unwrap(),
             amt)
        };
        if version != 1 {
            return Err(io::Error::new(io::ErrorKind::Other, "only HTTP/1.1 accepted"))
        }
        let data = src.split_to(amt).freeze();
        let mut ret = Request::builder();
        ret.method(&data[method.0..method.1]);
        ret.uri(data.slice(path.0, path.1));
        ret.version(http::Version::HTTP_11);
        for header in headers.iter() {
            let (k, v) = match *header {
                Some((ref k, ref v)) => (k, v),
                None => break,
            };
            let value = unsafe {
                HeaderValue::from_shared_unchecked(data.slice(v.0, v.1))
            };
            ret.header(&data[k.0..k.1], value);
        }

        let req = ret.body(()).map_err(|e| {
            io::Error::new(io::ErrorKind::Other, e)
        })?;
        Ok(Some(req))
    }
}

mod date {
    use std::cell::RefCell;
    use std::fmt::{self, Write};
    use std::str;

    use time::{self, Duration};

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
        next_update: time::Timespec,
    }

    thread_local!(static LAST: RefCell<LastRenderedNow> = RefCell::new(LastRenderedNow {
        bytes: [0; 128],
        amt: 0,
        next_update: time::Timespec::new(0, 0),
    }));

    impl fmt::Display for Now {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            LAST.with(|cache| {
                let mut cache = cache.borrow_mut();
                let now = time::get_time();
                if now >= cache.next_update {
                    cache.update(now);
                }
                f.write_str(cache.buffer())
            })
        }
    }

    impl LastRenderedNow {
        fn buffer(&self) -> &str {
            str::from_utf8(&self.bytes[..self.amt]).unwrap()
        }

        fn update(&mut self, now: time::Timespec) {
            self.amt = 0;
            write!(LocalBuffer(self), "{}", time::at(now).rfc822()).unwrap();
            self.next_update = now + Duration::seconds(1);
            self.next_update.nsec = 0;
        }
    }

    struct LocalBuffer<'a>(&'a mut LastRenderedNow);

    impl<'a> fmt::Write for LocalBuffer<'a> {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let start = self.0.amt;
            let end = start + s.len();
            self.0.bytes[start..end].copy_from_slice(s.as_bytes());
            self.0.amt += s.len();
            Ok(())
        }
    }
}
