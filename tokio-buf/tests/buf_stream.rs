use tokio_buf::BufStream;

// Ensures that `BufStream` can be a trait object
#[allow(dead_code)]
fn obj(_: &mut dyn BufStream<Item = u32, Error = ()>) {}
