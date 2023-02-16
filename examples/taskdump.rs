//! A service that emits task dumps to stdout upon receipt of a SIGUSR1 signal.

#![warn(rust_2018_idioms)]

use tokio::{
    time::{sleep, Duration},
    runtime::Handle,
    signal::unix::{signal, SignalKind}
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tokio::spawn(async {
        // Listen for SIGUSR1
        let mut stream = signal(SignalKind::user_defined1()).unwrap();
        loop {
            // Wait for receipt of SIGUSR1
            stream.recv().await;
            // Write a JSON taskdump to stdout.
            println!("{}", Handle::current());
        }
    });

    let sleep = sleep(Duration::from_secs(20));

    tokio::select! {
        _ = tokio::spawn(busy::work()) => {}
        _ = tokio::spawn(busy::work()) => {}
        _ = sleep => {/* exit the application */}
    };

    Ok(())
}

mod busy {
    use futures::future::{BoxFuture, FutureExt};

    #[inline(never)]
    pub async fn work() {
        loop {
            for i in 0..u8::MAX {
                recurse(i).await;
            }
        }
    }

    #[inline(never)]
    pub fn recurse(depth: u8) -> BoxFuture<'static, ()> {
        async move {
            tokio::task::yield_now().await;
            if depth == 0 {
                return;
            } else {
                recurse(depth - 1).await;
            }
        }
        .boxed()
    }
}
