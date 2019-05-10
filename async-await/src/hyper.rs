#![feature(await_macro, async_await)]

use tokio::async_wait;
use tokio::prelude::*;
use hyper::Client;

use std::time::Duration;
use std::str;

#[tokio::main]
async fn main() {
    let client = Client::new();

    let uri = "http://httpbin.org/ip".parse().unwrap();

    let response = async_wait!({
        client.get(uri)
            .timeout(Duration::from_secs(10))
    }).unwrap();

    println!("Response: {}", response.status());

    let mut body = response.into_body();

    while let Some(chunk) = async_wait!(body.next()) {
        let chunk = chunk.unwrap();
        println!("chunk = {}", str::from_utf8(&chunk[..]).unwrap());
    }
}
