use std::ops::{Deref, DerefMut};
use std::pin::Pin;

/// Similar to `DerefMut`, but dereferences `Pin` to `Pin`.
pub trait DerefPinMut: Deref {
    /// Deref.
    fn deref_pin(self: Pin<&mut Self>) -> Pin<&mut Self::Target>;
}

impl<T: ?Sized + Unpin> DerefPinMut for &mut T {
    fn deref_pin(self: Pin<&mut Self>) -> Pin<&mut T> {
        Pin::new(&mut *self.get_mut())
    }
}

impl<T: ?Sized + Unpin> DerefPinMut for Box<T> {
    fn deref_pin(self: Pin<&mut Self>) -> Pin<&mut T> {
        Pin::new(&mut *self.get_mut())
    }
}

impl<P> DerefPinMut for Pin<P>
where
    P: DerefMut + Unpin,
{
    fn deref_pin(self: Pin<&mut Self>) -> Pin<&mut P::Target> {
        self.get_mut().as_mut()
    }
}
