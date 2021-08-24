use tests_build::tokio;

#[tokio::main]
async fn missing_semicolon_or_return_type() {
    Ok(())
}

#[tokio::main]
async fn missing_return_type() {
    return Ok(());
}

#[tokio::main]
async fn extra_semicolon() -> Result<(), ()> {
    Ok(());
}

fn main() {}
