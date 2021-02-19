use std::ops::Deref;
use tokio::io::{AsyncRead, DerefPinMut};
use tokio::macros::support::Pin;

struct MyBox<A>(Box<A>);

impl<A> Deref for MyBox<A> {
    type Target = A;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl<A> DerefPinMut for MyBox<A>
where
    A: AsyncRead + Unpin,
{
    fn deref_pin(self: Pin<&mut Self>) -> Pin<&mut Self::Target> {
        Pin::new(&mut self.get_mut().0)
    }
}

fn _accepts_read<A: AsyncRead>(_: A) {}

fn _accepts_box<A: AsyncRead + Unpin>(b: MyBox<A>) {
    _accepts_read(b);
}

#[test]
fn test_deref_pin_mut_can_be_implemented_for_custom_type_to_aquire_async_read() {}
