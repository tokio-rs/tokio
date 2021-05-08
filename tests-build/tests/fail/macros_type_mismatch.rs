use tests_build::tokio;

#[tokio::main]
async fn missing_semicolon_or_return_type() {
    Ok(())
}

#[tokio::main]
async fn missing_return_type() {
    /* TODO(taiki-e): one of help messages still wrong
    help: consider using a semicolon here
       |
    16 |     return Ok(());;
       |
    */
    return Ok(());
}

#[tokio::main]
async fn extra_semicolon() -> Result<(), ()> {
    /* TODO(taiki-e): help message still wrong
    help: try using a variant of the expected enum
       |
    29 |     Ok(Ok(());)
       |
    29 |     Err(Ok(());)
       |
    */
    Ok(());
}

fn main() {}
