use std::error::Error;
use tokio::fs::walk_dir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut rx = walk_dir("./").await?; // awaiting this function starts
    while let Some(item) = rx.recv().await {
        println!("{:?}", item);
    }
    Ok(())
}
