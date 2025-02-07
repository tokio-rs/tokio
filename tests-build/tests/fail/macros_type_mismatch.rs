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

/// This test is a characterization test for the `?` operator.
///
/// See <https://github.com/tokio-rs/tokio/issues/6930#issuecomment-2572502517> for more details.
///
/// It should fail with a single error message about the return type of the function, but instead
/// if fails with an extra error message due to the `?` operator being used within the async block
/// rather than the original function.
///
/// ```text
/// 28 |     None?;
///    |         ^ cannot use the `?` operator in an async block that returns `()`
/// ```
#[tokio::main]
async fn question_mark_operator_with_invalid_option() -> Option<()> {
    None?;
}

/// This test is a characterization test for the `?` operator.
///
/// See <https://github.com/tokio-rs/tokio/issues/6930#issuecomment-2572502517> for more details.
///
/// It should fail with a single error message about the return type of the function, but instead
/// if fails with an extra error message due to the `?` operator being used within the async block
/// rather than the original function.
///
/// ```text
/// 33 |     Ok(())?;
///    |           ^ cannot use the `?` operator in an async block that returns `()`
/// ```
#[tokio::main]
async fn question_mark_operator_with_invalid_result() -> Result<(), ()> {
    Ok(())?;
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
