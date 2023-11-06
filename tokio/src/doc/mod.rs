//! Types which are documented locally in the Tokio crate, but does not actually
//! live here.
//!
//! **Note** this module is only visible on docs.rs, you cannot use it directly
//! in your own code.

/// The name of a type which is not defined here.
///
/// This is typically used as an alias for another type, like so:
///
/// ```rust,ignore
/// /// See [some::other::location](https://example.com).
/// type DEFINED_ELSEWHERE = crate::doc::NotDefinedHere;
/// ```
///
/// This type is uninhabitable like the [`never` type] to ensure that no one
/// will ever accidentally use it.
///
/// [`never` type]: https://doc.rust-lang.org/std/primitive.never.html
#[derive(Debug)]
pub enum NotDefinedHere {}

impl mio::event::Source for NotDefinedHere {
    fn register(
        &mut self,
        registry: &mio::Registry,
        token: mio::Token,
        interests: mio::Interest,
    ) -> std::io::Result<()> {
        Ok(())
    }
    fn reregister(
        &mut self,
        registry: &mio::Registry,
        token: mio::Token,
        interests: mio::Interest,
    ) -> std::io::Result<()> {
        Ok(())
    }
    fn deregister(&mut self, registry: &mio::Registry) -> std::io::Result<()> {
        Ok(())
    }
}

pub mod os;
