//! A simple example demonstrating how one might implement a custom
//! subscriber.
//!
//! This subscriber implements a tree-structured logger similar to
//! the "compact" formatter in [`slog-term`]. The demo mimicks the
//! example output in the screenshot in the [`slog` README].
//!
//! Note that this logger isn't ready for actual production use.
//! Several corners were cut to make the example simple.
//!
//! [`slog-term`]: https://docs.rs/slog-term/2.4.0/slog_term/
//! [`slog` README]: https://github.com/slog-rs/slog#terminal-output-example
#[macro_use]
extern crate tokio_trace;

use tokio_trace::{field, Level};

mod sloggish_subscriber;
use self::sloggish_subscriber::SloggishSubscriber;

fn main() {
    let subscriber = SloggishSubscriber::new(2);

    tokio_trace::subscriber::with_default(subscriber, || {
        span!(Level::TRACE, "", version = &field::display(5.0)).enter(|| {
            span!(Level::TRACE, "server", host = "localhost", port = 8080).enter(|| {
                info!("starting");
                info!("listening");
                let peer1 = span!(Level::TRACE, "conn", peer_addr = "82.9.9.9", port = 42381);
                peer1.enter(|| {
                    debug!("connected");
                    debug!({ length = 2 }, "message received");
                });
                let peer2 = span!(Level::TRACE, "conn", peer_addr = "8.8.8.8", port = 18230);
                peer2.enter(|| {
                    debug!("connected");
                });
                peer1.enter(|| {
                    warn!({ algo = "xor" }, "weak encryption requested");
                    debug!({ length = 8 }, "response sent");
                    debug!("disconnected");
                });
                peer2.enter(|| {
                    debug!({ length = 5 }, "message received");
                    debug!({ length = 8 }, "response sent");
                    debug!("disconnected");
                });
                warn!("internal error");
                info!("exit");
            })
        });
    });
}
