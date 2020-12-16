//! A "print-each-packet" server with Tokio
//!
//! This server will create a TCP listener, accept connections in a loop, and
//! put down in the stdout everything that's read off of each TCP connection.
//!
//! Because the Tokio runtime uses a thread pool, each TCP connection is
//! processed concurrently with all other TCP connections across multiple
//! threads.
//!
//! To see this server in action, you can run this in one terminal:
//!
//!     cargo run --example print\_each\_packet
//!
//! and in another terminal you can run:
//!
//!     cargo run --example connect 127.0.0.1:8080
//!
//! Each line you type in to the `connect` terminal should be written to terminal!
//!
//! Minimal js example:
//!
//! ```js
//! var net = require("net");
//!
//! var listenPort = 8080;
//!
//! var server = net.createServer(function (socket) {
//!     socket.on("data", function (bytes) {
//!         console.log("bytes", bytes);
//!     });
//!
//!     socket.on("end", function() {
//!         console.log("Socket received FIN packet and closed connection");
//!     });
//!     socket.on("error", function (error) {
//!         console.log("Socket closed with error", error);
//!     });
//!
//!     socket.on("close", function (with_error) {
//!         if (with_error) {
//!             console.log("Socket closed with result: Err(SomeError)");
//!         } else {
//!             console.log("Socket closed with result: Ok(())");
//!         }
//!     });
//!
//! });
//!
//! server.listen(listenPort);
//!
//! console.log("Listening on:", listenPort);
//! ```
//!

#![warn(rust_2018_idioms)]

use tokio::net::TcpListener;
use tokio_stream::StreamExt;
use tokio_util::codec::{BytesCodec, Decoder};

use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Allow passing an address to listen on as the first argument of this
    // program, but otherwise we'll just set up our TCP listener on
    // 127.0.0.1:8080 for connections.
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    // Next up we create a TCP listener which will listen for incoming
    // connections. This TCP listener is bound to the address we determined
    // above and must be associated with an event loop, so we pass in a handle
    // to our event loop. After the socket's created we inform that we're ready
    // to go and start accepting connections.
    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on: {}", addr);

    loop {
        // Asynchronously wait for an inbound socket.
        let (socket, _) = listener.accept().await?;

        // And this is where much of the magic of this server happens. We
        // crucially want all clients to make progress concurrently, rather than
        // blocking one on completion of another. To achieve this we use the
        // `tokio::spawn` function to execute the work in the background.
        //
        // Essentially here we're executing a new task to run concurrently,
        // which will allow all of our clients to be processed concurrently.
        tokio::spawn(async move {
            // We're parsing each socket with the `BytesCodec` included in `tokio::codec`.
            let mut framed = BytesCodec::new().framed(socket);

            // We loop while there are messages coming from the Stream `framed`.
            // The stream will return None once the client disconnects.
            while let Some(message) = framed.next().await {
                match message {
                    Ok(bytes) => println!("bytes: {:?}", bytes),
                    Err(err) => println!("Socket closed with error: {:?}", err),
                }
            }
            println!("Socket received FIN packet and closed connection");
        });
    }
}
