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

#[tokio::main]
async fn invalid_return_type() -> Option<()> {
    ()
}

// https://github.com/tokio-rs/tokio/issues/4635
#[allow(redundant_semicolons)]
#[rustfmt::skip]
#[tokio::main]
async fn issue_4635() {
    return 1;
    ;
}

fn main() {}
