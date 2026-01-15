use tests_build::tokio;

#[tokio::main]
async fn never() -> ! {
    loop {}
}

#[tokio::main]
async fn impl_trait() -> impl Iterator<Item = impl core::fmt::Debug> {
    [()].into_iter()
}

fn main() {
    if impl_trait().count() == 10 {
        never();
    }
}
