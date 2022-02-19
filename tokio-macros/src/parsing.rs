use core::fmt;

use proc_macro::{Delimiter, Spacing, Span, TokenTree};

const BUF: usize = 4;

// Punctuations that we look for.
pub(crate) const COMMA: [char; 2] = [',', '\0'];
pub(crate) const EQ: [char; 2] = ['=', '\0'];
pub(crate) const ARROW: [char; 2] = ['-', '>'];

pub(crate) struct Buf {
    // Static ring buffer used for processing tokens.
    ring: [Option<TokenTree>; BUF],
    head: usize,
    tail: usize,
    // Re-usable string buffer.
    string: String,
}

impl Buf {
    pub(crate) fn new() -> Self {
        Self {
            ring: [None, None, None, None],
            string: String::new(),
            head: 0,
            tail: 0,
        }
    }

    /// Clear the buffer.
    fn clear(&mut self) {
        self.ring = [None, None, None, None];
        self.head = 0;
        self.tail = 0;
        self.string.clear();
    }

    /// Get the next element out of the ring buffer.
    pub(crate) fn next(&mut self) -> Option<TokenTree> {
        if let Some(head) = self.ring.get_mut(self.tail % BUF).and_then(|s| s.take()) {
            self.tail += 1;
            Some(head)
        } else {
            None
        }
    }

    fn fill<I>(&mut self, n: usize, mut it: I) -> Option<()>
    where
        I: Iterator<Item = TokenTree>,
    {
        assert!(n <= BUF);

        while (self.head - self.tail) <= n {
            self.ring[self.head % BUF] = Some(it.next()?);
            self.head += 1;
        }

        Some(())
    }

    /// Try to get the `n`th token and fill from the provided iterator if neede.
    pub(crate) fn nth<I>(&mut self, n: usize, it: I) -> Option<&TokenTree>
    where
        I: Iterator<Item = TokenTree>,
    {
        self.fill(n, it)?;
        self.ring.get((self.tail + n) % BUF)?.as_ref()
    }

    /// Coerce the given value into a string by formatting into an existing
    /// string buffer.
    pub(crate) fn display_as_str(&mut self, value: impl fmt::Display) -> &str {
        use std::fmt::Write;

        self.string.clear();
        let _ = write!(&mut self.string, "{}", value);
        self.string.as_str()
    }
}

/// Parser base.
pub(crate) struct BaseParser<'a> {
    it: StreamIter,
    tokens: Vec<TokenTree>,
    pub(crate) buf: &'a mut Buf,
}

impl<'a> BaseParser<'a> {
    pub(crate) fn new(stream: proc_macro::TokenStream, buf: &'a mut Buf) -> Self {
        buf.clear();

        Self {
            it: StreamIter::new(stream),
            tokens: Vec::new(),
            buf,
        }
    }

    /// Push a single token onto the token buffer.
    pub(crate) fn push(&mut self, tt: TokenTree) {
        self.tokens.push(tt);
    }

    /// The current length in number of tokens recorded.
    pub(crate) fn len(&self) -> usize {
        self.tokens.len()
    }

    /// Access the token at the given offset.
    pub(crate) fn nth(&mut self, n: usize) -> Option<&TokenTree> {
        self.buf.nth(n, &mut self.it)
    }

    /// Bump the last token.
    pub(crate) fn bump(&mut self) -> Option<TokenTree> {
        if let Some(head) = self.buf.next() {
            return Some(head);
        }

        self.it.next()
    }

    /// Step over the given number of tokens.
    pub(crate) fn step(&mut self, n: usize) {
        for _ in 0..n {
            self.bump();
        }
    }

    /// Step over the given number of tokens.
    pub(crate) fn consume(&mut self, n: usize) {
        for _ in 0..n {
            if let Some(tt) = self.bump() {
                self.push(tt);
            }
        }
    }

    /// Peek a punctuation with joint characters.
    ///
    /// This processes the next 3 punctuations (if present) to ensure that when
    /// we encounter a particular punctuation it occurs in isolation.
    pub(crate) fn peek_punct(&mut self) -> Option<Punct> {
        let mut out = [None; 3];

        for (n, o) in out.iter_mut().enumerate() {
            match (n, self.nth(n)) {
                (_, Some(TokenTree::Punct(punct))) => {
                    *o = Some((punct.span(), punct.as_char()));

                    if !matches!(punct.spacing(), Spacing::Joint) {
                        break;
                    }
                }
                _ => {
                    break;
                }
            }
        }

        match out {
            [Some((span, head)), tail, None] => Some(Punct {
                span,
                chars: [head, tail.map(|(_, c)| c).unwrap_or('\0')],
            }),
            _ => None,
        }
    }

    /// Skip the specified punctuations and return a boolean indicating if it was skipped.
    pub(crate) fn skip_punct(&mut self, expected: [char; 2]) -> bool {
        if let Some(p) = self.peek_punct() {
            if p.chars == expected {
                self.step(p.len());
                return true;
            }
        }

        false
    }

    /// Convert the current parser into a collection of tokens it has retained.
    pub(crate) fn into_tokens_and_buf(self) -> (Vec<TokenTree>, &'a mut Buf) {
        (self.tokens, self.buf)
    }
}

struct StreamIter {
    stack: Vec<proc_macro::token_stream::IntoIter>,
}

impl StreamIter {
    fn new(stream: proc_macro::TokenStream) -> Self {
        Self {
            stack: vec![stream.into_iter()],
        }
    }
}

impl Iterator for StreamIter {
    type Item = TokenTree;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(tt) = self.stack.last_mut()?.next() {
                return match tt {
                    TokenTree::Group(g) if g.delimiter() == Delimiter::None => {
                        self.stack.push(g.stream().into_iter());
                        continue;
                    }
                    tt => Some(tt),
                };
            }

            let _ = self.stack.pop();
        }
    }
}

/// A complete punctuation with a maximum up to two characters.
#[derive(Debug)]
pub(crate) struct Punct {
    pub(crate) span: Span,
    pub(crate) chars: [char; 2],
}

impl Punct {
    pub(crate) fn len(&self) -> usize {
        self.chars.iter().take_while(|c| **c != '\0').count()
    }
}
