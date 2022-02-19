mod output;
pub(crate) use self::output::{EntryKind, SupportsThreading};

mod parser;

use crate::error::Error;
use crate::into_tokens::{from_fn, IntoTokens};
use crate::parsing::Buf;
use crate::token_stream::TokenStream;

/// Url printed when we failed to expand, but we haven't provided end-user
/// workable diagnostics for why.
const REPORT_URL: &str = "https://github.com/tokio-rs/tokio/issues/new/choose";

/// Configurable macro code to build entry.
pub(crate) fn build(
    kind: EntryKind,
    supports_threading: SupportsThreading,
    args: proc_macro::TokenStream,
    item_stream: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let mut buf = Buf::new();
    let mut errors = Vec::new();

    let config = parser::ConfigParser::new(args, &mut buf, &mut errors);
    let config = config.parse(kind, supports_threading);

    config.validate(kind, &mut errors, &mut buf);

    let item = parser::ItemParser::new(item_stream, &mut buf);
    let item = item.parse();

    item.validate(kind, &mut errors);

    let mut stream = TokenStream::default();

    let (start, end) = item.block_spans();

    if let Some(item) = item.expand_item(kind, config, start) {
        item.into_tokens(&mut stream, end);
    } else {
        // Report a general "failed to do the thing" error to ensure we're never
        // completely silent.
        //
        // If this is encountered in the wild, it should be troubleshot to
        // ensure we're providing good diagnostics for whatever failed.
        if errors.is_empty() {
            errors.push(Error::new(
                item.error_span(),
                format!(
                    "#[{}] failed to process function, please report this as a bug to {}",
                    kind.name(),
                    REPORT_URL
                ),
            ));
        }

        item.expand_fallback().into_tokens(&mut stream, end);
    }

    format_item_errors(errors).into_tokens(&mut stream, end);

    stream.into_token_stream()
}

fn format_item_errors<I>(errors: I) -> impl IntoTokens
where
    I: IntoIterator<Item = Error>,
{
    from_fn(move |s| {
        for error in errors {
            s.write(error);
        }
    })
}
