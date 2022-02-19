use std::ops;

use proc_macro::{Delimiter, Span, TokenTree};

use crate::error::Error;
use crate::into_tokens::{bracketed, from_fn, parens, string, IntoTokens, S};
use crate::parsing::Buf;
use crate::token_stream::TokenStream;

#[derive(Debug, Clone, Copy)]
pub(crate) enum ReturnHeuristics {
    /// Unknown how to treat the return type.
    Unknown,
    /// Generated function explicitly returns the special `()` unit type.
    Unit,
}

/// The kind of the tail expression.
#[derive(Debug, Clone, Copy)]
pub(crate) enum TailKind {
    /// Body is empty.
    Empty,
    /// Tail is a return statement (prefix `return`).
    Return,
    /// Body is non-empty but the tail is not specifically recognized.
    Unknown,
}

impl Default for TailKind {
    fn default() -> Self {
        TailKind::Empty
    }
}

#[derive(Default)]
pub(crate) struct Tail {
    /// The start span of the tail.
    pub(crate) start: Option<Span>,
    /// The end span of the tail including a trailing semi.
    pub(crate) end: Option<Span>,
    /// Indicates if last expression is a return.
    pub(crate) kind: TailKind,
    /// If the tail statement has a semi-colon.
    pub(crate) has_semi: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum EntryKind {
    // Because of how all entries in this crate that use this are marked with
    // `#[cfg(not(test))]` this yields a warning when performing a test build.
    #[allow(unused)]
    Main,
    Test,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SupportsThreading {
    Supported,
    NotSupported,
}

impl EntryKind {
    /// The name of the attribute used as the entry kind.
    pub(crate) fn name(&self) -> &str {
        match self {
            EntryKind::Main => "tokio::main",
            EntryKind::Test => "tokio::test",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RuntimeFlavor {
    CurrentThread,
    Threaded,
}

impl RuntimeFlavor {
    /// Parse a literal (as it appears in Rust code) as a runtime flavor. This
    /// means that it includes quotes.
    pub(crate) fn from_literal(s: &str) -> Result<RuntimeFlavor, &'static str> {
        match s {
            "\"current_thread\"" => Ok(RuntimeFlavor::CurrentThread),
            "\"multi_thread\"" => Ok(RuntimeFlavor::Threaded),
            "\"single_thread\"" => Err("the single threaded runtime flavor is called \"current_thread\""),
            "\"basic_scheduler\"" => Err("the \"basic_scheduler\" runtime flavor has been renamed to \"current_thread\""),
            "\"threaded_scheduler\"" => Err("the \"threaded_scheduler\" runtime flavor has been renamed to \"multi_thread\""),
            _ => Err("no such runtime flavor, the runtime flavors are: \"current_thread\", \"multi_thread\""),
        }
    }
}

/// The parsed arguments output.
#[derive(Debug)]
pub(crate) struct Config {
    pub(crate) supports_threading: SupportsThreading,
    /// The default runtime flavor to use if left unspecified.
    default_flavor: Option<RuntimeFlavor>,
    /// The runtime flavor to use.
    pub(crate) flavor: Option<(Span, RuntimeFlavor)>,
    /// The number of worker threads to configure.
    pub(crate) worker_threads: Option<TokenTree>,
    /// If the runtime should start paused.
    pub(crate) start_paused: Option<TokenTree>,
}

impl Config {
    pub(crate) fn new(kind: EntryKind, supports_threading: SupportsThreading) -> Self {
        Self {
            supports_threading,
            default_flavor: match (kind, supports_threading) {
                (EntryKind::Main, SupportsThreading::Supported) => Some(RuntimeFlavor::Threaded),
                (EntryKind::Main, SupportsThreading::NotSupported) => None,
                (EntryKind::Test, _) => Some(RuntimeFlavor::CurrentThread),
            },
            flavor: None,
            worker_threads: None,
            start_paused: None,
        }
    }

    /// Validate the current configuration.
    pub(crate) fn validate(&self, kind: EntryKind, errors: &mut Vec<Error>, buf: &mut Buf) {
        if let (None, SupportsThreading::NotSupported) =
            (self.default_flavor, self.supports_threading)
        {
            errors.push(Error::new(Span::call_site(), "the default runtime flavor is `multi_thread`, but the `rt-multi-thread` feature is disabled"))
        }

        if let (Some(RuntimeFlavor::Threaded), Some(tt)) = (self.flavor(), &self.start_paused) {
            if buf.display_as_str(tt) == "true" {
                errors.push(Error::new(tt.span(), format!("the `start_paused` option requires the \"current_thread\" runtime flavor. Use `#[{}(flavor = \"current_thread\")]`", kind.name())));
            }
        }

        if let (Some(RuntimeFlavor::CurrentThread), Some(tt)) =
            (self.flavor(), &self.worker_threads)
        {
            errors.push(Error::new(tt.span(), format!("the `worker_threads` option requires the \"multi_thread\" runtime flavor. Use `#[{}(flavor = \"multi_thread\")]`", kind.name())));
        }
    }

    /// Get the runtime flavor to use.
    fn flavor(&self) -> Option<RuntimeFlavor> {
        match &self.flavor {
            Some((_, flavor)) => Some(*flavor),
            None => self.default_flavor,
        }
    }
}

/// The parsed item output.
pub(crate) struct ItemOutput {
    tokens: Vec<TokenTree>,
    async_keyword: Option<usize>,
    signature: Option<ops::Range<usize>>,
    block: Option<usize>,
    /// What's known about the tail statement.
    tail: Tail,
    /// Best effort heuristics to determine the return value of the function being procssed.
    #[allow(unused)]
    return_heuristics: ReturnHeuristics,
}

impl ItemOutput {
    pub(crate) fn new(
        tokens: Vec<TokenTree>,
        async_keyword: Option<usize>,
        signature: Option<ops::Range<usize>>,
        block: Option<usize>,
        tail: Tail,
        return_heuristics: ReturnHeuristics,
    ) -> Self {
        Self {
            tokens,
            async_keyword,
            signature,
            block,
            tail,
            return_heuristics,
        }
    }

    /// Validate the parsed item.
    pub(crate) fn validate(&self, kind: EntryKind, errors: &mut Vec<Error>) {
        if self.async_keyword.is_none() {
            let span = self
                .signature
                .as_ref()
                .and_then(|s| self.tokens.get(s.clone()))
                .and_then(|t| t.first())
                .map(|tt| tt.span())
                .unwrap_or_else(Span::call_site);

            errors.push(Error::new(
                span,
                format!("functions marked with `#[{}]` must be `async`", kind.name()),
            ));
        }
    }

    /// Calculate the block span to use for diagnostics. This will correspond to
    /// the last tail statement in the block of the function body.
    pub(crate) fn block_spans(&self) -> (Span, Span) {
        let fallback_span = self
            .block
            .and_then(|index| Some(self.tokens.get(index)?.span()));
        let start = self
            .tail
            .start
            .or(fallback_span)
            .unwrap_or_else(Span::call_site);
        let end = self
            .tail
            .end
            .or(fallback_span)
            .unwrap_or_else(Span::call_site);
        (start, end)
    }

    /// Expand into a function item.
    pub(crate) fn expand_item(
        &self,
        kind: EntryKind,
        config: Config,
        start: Span,
    ) -> impl IntoTokens + '_ {
        from_fn(move |s| {
            if let Some(item) = self.maybe_expand_item(kind, config, start) {
                s.write(item);
            } else {
                s.write(&self.tokens[..]);
            }
        })
    }

    /// Expand item if all prerequsites are available.
    fn maybe_expand_item(
        &self,
        kind: EntryKind,
        config: Config,
        start: Span,
    ) -> Option<impl IntoTokens + '_> {
        let signature = self.tokens.get(self.signature.as_ref()?.clone())?;
        let block = self.tokens.get(self.block?)?;
        let flavor = config.flavor()?;

        Some((
            self.entry_kind_attribute(kind),
            from_fn(move |s| {
                // Optionally filter the async keyword (if it's present). We
                // still want to be able to produce a signature cause we get
                // better diagnostics.
                if let Some(index) = self.async_keyword {
                    for (n, tt) in signature.iter().enumerate() {
                        if n != index {
                            s.write(tt.clone());
                        }
                    }
                } else {
                    s.write(signature);
                }
            }),
            group_with_span(
                Delimiter::Brace,
                self.item_body(config, block, flavor, start),
                block.span(),
            ),
        ))
    }

    /// Generate attribute associated with entry kind.
    fn entry_kind_attribute(&self, kind: EntryKind) -> impl IntoTokens {
        from_fn(move |s| {
            if let EntryKind::Test = kind {
                s.write((
                    '#',
                    bracketed((S, "core", S, "prelude", S, "v1", S, "test")),
                ))
            }
        })
    }

    /// Expanded item body.
    fn item_body<'a>(
        &'a self,
        config: Config,
        block: &'a TokenTree,
        flavor: RuntimeFlavor,
        start: Span,
    ) -> impl IntoTokens + 'a {
        // NB: override the first generated part with the detected start span.
        let rt = ("tokio", S, "runtime", S, "Builder");

        let rt = from_fn(move |s| {
            s.write(rt);

            match flavor {
                RuntimeFlavor::CurrentThread => {
                    s.write((S, "new_current_thread", parens(())));
                }
                RuntimeFlavor::Threaded => {
                    s.write((S, "new_multi_thread", parens(())));
                }
            }

            if let Some(start_paused) = config.start_paused {
                s.write(('.', "start_paused", parens(start_paused)));
            }

            if let Some(worker_threads) = config.worker_threads {
                s.write(('.', "worker_threads", parens(worker_threads)));
            }
        });

        let build = (
            (rt, '.', "enable_all", parens(()), '.', "build", parens(())),
            '.',
            "expect",
            parens(string("Failed building the Runtime")),
        );

        let statement = (
            with_span((build, '.', "block_on"), start),
            parens(("async", block.clone())),
        );

        let has_return = self.tail.has_semi && matches!(self.tail.kind, TailKind::Return);

        let has_semi =
            if !has_return && (self.tail.has_semi || matches!(self.tail.kind, TailKind::Empty)) {
                matches!(self.return_heuristics, ReturnHeuristics::Unit)
            } else {
                false
            };

        from_fn(move |s| {
            if has_return {
                s.write(with_span("return", start));
            }

            s.write(statement);

            if has_semi {
                s.write(';');
            }
        })
    }
}

/// Insert the given tokens with a custom span.
pub(crate) fn with_span<T>(inner: T, span: Span) -> impl IntoTokens
where
    T: IntoTokens,
{
    WithSpan(inner, span)
}

struct WithSpan<T>(T, Span);

impl<T> IntoTokens for WithSpan<T>
where
    T: IntoTokens,
{
    fn into_tokens(self, stream: &mut TokenStream, _: Span) {
        self.0.into_tokens(stream, self.1);
    }
}

/// Construct a custom group  with a custom span that is not inherited by its
/// children.
fn group_with_span<T>(delimiter: Delimiter, inner: T, span: Span) -> impl IntoTokens
where
    T: IntoTokens,
{
    GroupWithSpan(delimiter, inner, span)
}

struct GroupWithSpan<T>(Delimiter, T, Span);

impl<T> IntoTokens for GroupWithSpan<T>
where
    T: IntoTokens,
{
    fn into_tokens(self, stream: &mut TokenStream, span: Span) {
        let checkpoint = stream.checkpoint();
        self.1.into_tokens(stream, span);
        stream.group(self.2, self.0, checkpoint);
    }
}
