use tests_build::tokio;

fn main() {}

// arguments and output type is forwarded so other macros can access them

#[tokio::test]
async fn test_fn_has_args(_x: u8) {}

#[tokio::test]
async fn test_has_output() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
