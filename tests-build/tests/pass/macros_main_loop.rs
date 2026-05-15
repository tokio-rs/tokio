use tests_build::tokio;

#[tokio::main]
async fn main() -> Result<(), ()> {
    loop {
        if !never() {
            return Ok(());
        }
    }
}

fn never() -> bool {
    std::time::Instant::now() > std::time::Instant::now()
}
