//! A "tiny database" and accompanying protocol
//!
//! This example shows the usage of shared state amongst all connected clients,
//! namely a database of key/value pairs. Each connected client can send a
//! series of GET/SET commands to query the current value of a key or set the
//! value of a key.
//!
//! This example has a simple protocol you can use to interact with the server.
//! To run, first run this in one terminal window:
//!
//!     cargo run --example tinydb
//!
//! and next in another windows run:
//!
//!     cargo run --example connect 127.0.0.1:8080
//!
//! In the `connect` window you can type in commands where when you hit enter
//! you'll get a response from the server for that command. An example session
//! is:
//!
//!
//!     $ cargo run --example connect 127.0.0.1:8080
//!     GET foo
//!     foo = bar
//!     GET FOOBAR
//!     error: no key FOOBAR
//!     SET FOOBAR my awesome string
//!     set FOOBAR = `my awesome string`, previous: None
//!     SET foo tokio
//!     set foo = `tokio`, previous: Some("bar")
//!     GET foo
//!     foo = tokio
//!
//! Namely you can issue two forms of commands:
//!
//! * `GET $key` - this will fetch the value of `$key` from the database and
//!   return it. The server's database is initially populated with the key `foo`
//!   set to the value `bar`
//! * `SET $key $value` - this will set the value of `$key` to `$value`,
//!   returning the previous value, if any.

#![deny(warnings)]

extern crate tokio;

use std::collections::HashMap;
use std::env;
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::io::{lines, write_all};
use tokio::net::TcpListener;
use tokio::prelude::*;

/// The in-memory database shared amongst all clients.
///
/// This database will be shared via `Arc`, so to mutate the internal map we're
/// going to use a `Mutex` for interior mutability.
struct Database {
    map: Mutex<HashMap<String, String>>,
}

/// Possible requests our clients can send us
enum Request {
    Get { key: String },
    Set { key: String, value: String },
}

/// Responses to the `Request` commands above
enum Response {
    Value {
        key: String,
        value: String,
    },
    Set {
        key: String,
        value: String,
        previous: Option<String>,
    },
    Error {
        msg: String,
    },
}

fn main() -> Result<(), Box<std::error::Error>> {
    // Parse the address we're going to run this server on
    // and set up our TCP listener to accept connections.
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>()?;
    let listener = TcpListener::bind(&addr).map_err(|_| "failed to bind")?;
    println!("Listening on: {}", addr);

    // Create the shared state of this server that will be shared amongst all
    // clients. We populate the initial database and then create the `Database`
    // structure. Note the usage of `Arc` here which will be used to ensure that
    // each independently spawned client will have a reference to the in-memory
    // database.
    let mut initial_db = HashMap::new();
    initial_db.insert("foo".to_string(), "bar".to_string());
    let db = Arc::new(Database {
        map: Mutex::new(initial_db),
    });

    let done = listener
        .incoming()
        .map_err(|e| println!("error accepting socket; error = {:?}", e))
        .for_each(move |socket| {
            // As with many other small examples, the first thing we'll do is
            // *split* this TCP stream into two separately owned halves. This'll
            // allow us to work with the read and write halves independently.
            let (reader, writer) = socket.split();

            // Since our protocol is line-based we use `tokio_io`'s `lines` utility
            // to convert our stream of bytes, `reader`, into a `Stream` of lines.
            let lines = lines(BufReader::new(reader));

            // Here's where the meat of the processing in this server happens. First
            // we see a clone of the database being created, which is creating a
            // new reference for this connected client to use. Also note the `move`
            // keyword on the closure here which moves ownership of the reference
            // into the closure, which we'll need for spawning the client below.
            //
            // The `map` function here means that we'll run some code for all
            // requests (lines) we receive from the client. The actual handling here
            // is pretty simple, first we parse the request and if it's valid we
            // generate a response based on the values in the database.
            let db = db.clone();
            let responses = lines.map(move |line| {
                let request = match Request::parse(&line) {
                    Ok(req) => req,
                    Err(e) => return Response::Error { msg: e },
                };

                let mut db = db.map.lock().unwrap();
                match request {
                    Request::Get { key } => match db.get(&key) {
                        Some(value) => Response::Value {
                            key,
                            value: value.clone(),
                        },
                        None => Response::Error {
                            msg: format!("no key {}", key),
                        },
                    },
                    Request::Set { key, value } => {
                        let previous = db.insert(key.clone(), value.clone());
                        Response::Set {
                            key,
                            value,
                            previous,
                        }
                    }
                }
            });

            // At this point `responses` is a stream of `Response` types which we
            // now want to write back out to the client. To do that we use
            // `Stream::fold` to perform a loop here, serializing each response and
            // then writing it out to the client.
            let writes = responses.fold(writer, |writer, response| {
                let mut response = response.serialize();
                response.push('\n');
                write_all(writer, response.into_bytes()).map(|(w, _)| w)
            });

            // Like with other small servers, we'll `spawn` this client to ensure it
            // runs concurrently with all other clients, for now ignoring any errors
            // that we see.
            let msg = writes.then(move |_| Ok(()));

            tokio::spawn(msg)
        });

    tokio::run(done);
    Ok(())
}

impl Request {
    fn parse(input: &str) -> Result<Request, String> {
        let mut parts = input.splitn(3, " ");
        match parts.next() {
            Some("GET") => {
                let key = match parts.next() {
                    Some(key) => key,
                    None => return Err(format!("GET must be followed by a key")),
                };
                if parts.next().is_some() {
                    return Err(format!("GET's key must not be followed by anything"));
                }
                Ok(Request::Get {
                    key: key.to_string(),
                })
            }
            Some("SET") => {
                let key = match parts.next() {
                    Some(key) => key,
                    None => return Err(format!("SET must be followed by a key")),
                };
                let value = match parts.next() {
                    Some(value) => value,
                    None => return Err(format!("SET needs a value")),
                };
                Ok(Request::Set {
                    key: key.to_string(),
                    value: value.to_string(),
                })
            }
            Some(cmd) => Err(format!("unknown command: {}", cmd)),
            None => Err(format!("empty input")),
        }
    }
}

impl Response {
    fn serialize(&self) -> String {
        match *self {
            Response::Value { ref key, ref value } => format!("{} = {}", key, value),
            Response::Set {
                ref key,
                ref value,
                ref previous,
            } => format!("set {} = `{}`, previous: {:?}", key, value, previous),
            Response::Error { ref msg } => format!("error: {}", msg),
        }
    }
}
