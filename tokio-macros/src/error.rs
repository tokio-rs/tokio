use core::fmt;
use core::iter::once;
use core::iter::FromIterator;

use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenTree};

use crate::to_tokens::ToTokens;
use crate::token_stream::TokenStream;

/// Expand a message as an error.
pub(crate) fn expand(message: &str) -> proc_macro::TokenStream {
    let error = Error::new(Span::call_site(), message);
    let mut stream = TokenStream::default();
    error.to_tokens(&mut stream, Span::call_site());
    stream.into_token_stream()
}

/// An error that can be raised during parsing which is associated with a span.
#[derive(Debug)]
pub(crate) struct Error {
    span: Span,
    message: Box<str>,
}

impl Error {
    pub(crate) fn new(span: Span, message: impl fmt::Display) -> Self {
        Self {
            span,
            message: message.to_string().into(),
        }
    }
}

impl ToTokens for Error {
    fn to_tokens(self, stream: &mut TokenStream, _: Span) {
        stream.push(TokenTree::Ident(Ident::new("compile_error", self.span)));
        let mut exclamation = Punct::new('!', Spacing::Alone);
        exclamation.set_span(self.span);
        stream.push(TokenTree::Punct(exclamation));

        let mut message = Literal::string(self.message.as_ref());
        message.set_span(self.span);

        let message = proc_macro::TokenStream::from_iter(once(TokenTree::Literal(message)));
        let mut group = Group::new(Delimiter::Brace, message);
        group.set_span(self.span);

        stream.push(TokenTree::Group(group));
    }
}
