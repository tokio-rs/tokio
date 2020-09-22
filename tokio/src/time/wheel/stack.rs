use std::borrow::Borrow;

/// Abstracts the stack operations needed to track timeouts.
pub(crate) trait Stack: Default {
    /// Type of the item stored in the stack
    type Owned: Borrow<Self::Borrowed>;

    /// Borrowed item
    type Borrowed;

    /// Returns `true` if the stack is empty
    fn is_empty(&self) -> bool;

    /// Push an item onto the stack
    fn push(&mut self, item: Self::Owned);

    /// Pop an item from the stack
    fn pop(&mut self) -> Option<Self::Owned>;

    fn remove(&mut self, item: &Self::Borrowed);

    fn when(item: &Self::Borrowed) -> u64;
}
