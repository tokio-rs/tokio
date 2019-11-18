#![warn(rust_2018_idioms)]

// A tiny async TLS echo server with Tokio
use tokio;

#[cfg(all(feature = "rustls", not(feature = "native-tls")))]
mod echo_rustls;
#[cfg(all(feature = "rustls", not(feature = "native-tls")))]
use echo_rustls::runnable as echo;

#[cfg(feature = "native-tls")]
mod echo_nativetls;
#[cfg(feature = "native-tls")]
use echo_nativetls::runnable as echo;

#[cfg(all(not(feature = "rustls"), not(feature = "native-tls")))]
mod echo {
    pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        panic!(
            "You must have either the rustls or native-tls feature enabled to run this example."
        );
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    echo::run().await
}
