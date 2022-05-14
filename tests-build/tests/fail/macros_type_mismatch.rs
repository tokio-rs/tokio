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
    /* TODO(taiki-e): help message still wrong
    help: try using a variant of the expected enum
       |
    23 |     Ok(Ok(());)
       |
    23 |     Err(Ok(());)
       |
    */
    Ok(());
}

fn main() {}
