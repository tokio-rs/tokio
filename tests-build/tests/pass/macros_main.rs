use tests_build::tokio;

// This ensures that `'static>>` is not being incorrectly consumed as one
// sequence of joint tokens while parsing the angle bracket group.
#[tokio::main]
async fn ensure_proper_continuation() -> Result<(), Box<dyn std::error::Error + 'static>> {
    todo!()
}

fn main() {}
