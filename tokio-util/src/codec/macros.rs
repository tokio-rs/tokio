/// A macro to reduce some of the boilerplate for projecting from
/// `Pin<&mut T>` to `Pin<&mut T.field>`
macro_rules! pin {
    ($e:expr) => {
        std::pin::Pin::new(&mut $e)
    };
}
