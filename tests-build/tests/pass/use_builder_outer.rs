#![deny(unused_qualifications)]

use tests_build::tokio;
pub use tokio::runtime;

#[tokio::main]
async fn main() {
    if true {}
}
