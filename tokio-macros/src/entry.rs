use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::spanned::Spanned;

#[derive(Clone, Copy, PartialEq)]
enum RuntimeFlavor {
    CurrentThread,
    Threaded,
}

impl RuntimeFlavor {
    fn from_str(s: &str) -> Result<RuntimeFlavor, String> {
        match s {
            "current_thread" => Ok(RuntimeFlavor::CurrentThread),
            "threaded" => Ok(RuntimeFlavor::Threaded),
            "single_threaded" => Err("The single threaded runtime flavor is called `current_thread`.".to_string()),
            "basic_scheduler" => Err("The `basic_scheduler` runtime flavor has been renamed to `current_thread`.".to_string()),
            "threaded_scheduler" => Err("The `threaded_scheduler` runtime flavor has been renamed to `threaded`.".to_string()),
            _ => Err(format!("No such runtime flavor `{}`. The runtime flavors are `current_thread` and `threaded`.", s)),
        }
    }
}

struct FinalConfig {
    flavor: RuntimeFlavor,
    worker_threads: Option<usize>,
}

struct Configuration {
    rt_threaded_available: bool,
    default_flavor: RuntimeFlavor,
    flavor: Option<RuntimeFlavor>,
    worker_threads: Option<(usize, Span)>,
}

impl Configuration {
    fn new(is_test: bool, rt_threaded: bool) -> Self {
        Configuration {
            rt_threaded_available: rt_threaded,
            default_flavor: match is_test {
                true => RuntimeFlavor::CurrentThread,
                false => RuntimeFlavor::Threaded,
            },
            flavor: None,
            worker_threads: None,
        }
    }

    fn set_flavor(&mut self, runtime: syn::Lit, span: Span) -> Result<(), syn::Error> {
        if self.flavor.is_some() {
            return Err(syn::Error::new(span, "`flavor` set multiple times."));
        }

        let runtime_str = parse_string(runtime, span, "flavor")?;
        let runtime =
            RuntimeFlavor::from_str(&runtime_str).map_err(|err| syn::Error::new(span, err))?;
        self.flavor = Some(runtime);
        Ok(())
    }

    fn set_worker_threads(
        &mut self,
        worker_threads: syn::Lit,
        span: Span,
    ) -> Result<(), syn::Error> {
        if self.worker_threads.is_some() {
            return Err(syn::Error::new(
                span,
                "`worker_threads` set multiple times.",
            ));
        }

        let worker_threads = parse_int(worker_threads, span, "worker_threads")?;
        if worker_threads == 0 {
            return Err(syn::Error::new(span, "`worker_threads` may not be 0."));
        }
        self.worker_threads = Some((worker_threads, span));
        Ok(())
    }

    fn build(&self) -> Result<FinalConfig, syn::Error> {
        let flavor = self.flavor.unwrap_or(self.default_flavor);
        use RuntimeFlavor::*;
        match (flavor, self.worker_threads) {
            (CurrentThread, Some((_, worker_threads_span))) => Err(syn::Error::new(
                worker_threads_span,
                "The `worker_threads` option requires the `threaded` runtime flavor.",
            )),
            (CurrentThread, None) => Ok(FinalConfig {
                flavor,
                worker_threads: None,
            }),
            (Threaded, worker_threads) if self.rt_threaded_available => Ok(FinalConfig {
                flavor,
                worker_threads: worker_threads.map(|(val, _span)| val),
            }),
            (Threaded, _) => {
                let msg = if self.flavor.is_none() {
                    "The default runtime flavor is `threaded`, but the `rt-threaded` feature is disabled."
                } else {
                    "The runtime flavor `threaded` requires the `rt-threaded` feature."
                };
                Err(syn::Error::new(Span::call_site(), msg))
            }
        }
    }
}

fn parse_int(int: syn::Lit, span: Span, field: &str) -> Result<usize, syn::Error> {
    match int {
        syn::Lit::Int(lit) => match lit.base10_parse::<usize>() {
            Ok(value) => Ok(value),
            Err(e) => Err(syn::Error::new(
                span,
                format!("Failed to parse {} as integer: {}", field, e),
            )),
        },
        _ => Err(syn::Error::new(
            span,
            format!("Failed to parse {} as integer.", field),
        )),
    }
}

fn parse_string(int: syn::Lit, span: Span, field: &str) -> Result<String, syn::Error> {
    match int {
        syn::Lit::Str(s) => Ok(s.value()),
        syn::Lit::Verbatim(s) => Ok(s.to_string()),
        _ => Err(syn::Error::new(
            span,
            format!("Failed to parse {} as string.", field),
        )),
    }
}

fn parse_knobs(
    mut input: syn::ItemFn,
    args: syn::AttributeArgs,
    is_test: bool,
    rt_threaded: bool,
) -> Result<TokenStream, syn::Error> {
    let sig = &mut input.sig;
    let body = &input.block;
    let attrs = &input.attrs;
    let vis = input.vis;

    if sig.asyncness.is_none() {
        let msg = "the async keyword is missing from the function declaration";
        return Err(syn::Error::new_spanned(sig.fn_token, msg));
    }

    sig.asyncness = None;

    let macro_name = if is_test {
        "tokio::test"
    } else {
        "tokio::main"
    };
    let mut config = Configuration::new(is_test, rt_threaded);

    for arg in args {
        match arg {
            syn::NestedMeta::Meta(syn::Meta::NameValue(namevalue)) => {
                let ident = namevalue.path.get_ident();
                if ident.is_none() {
                    let msg = "Must have specified ident";
                    return Err(syn::Error::new_spanned(namevalue, msg));
                }
                match ident.unwrap().to_string().to_lowercase().as_str() {
                    "worker_threads" => {
                        config.set_worker_threads(namevalue.lit.clone(), namevalue.span())?;
                    }
                    "flavor" => {
                        config.set_flavor(namevalue.lit.clone(), namevalue.span())?;
                    }
                    "core_threads" => {
                        let msg = "Attribute `core_threads` is renamed to `worker_threads`";
                        return Err(syn::Error::new_spanned(namevalue, msg));
                    }
                    name => {
                        let msg = format!("Unknown attribute {} is specified; expected one of: `flavor`, `worker_threads`, `max_threads`", name);
                        return Err(syn::Error::new_spanned(namevalue, msg));
                    }
                }
            }
            syn::NestedMeta::Meta(syn::Meta::Path(path)) => {
                let ident = path.get_ident();
                if ident.is_none() {
                    let msg = "Must have specified ident";
                    return Err(syn::Error::new_spanned(path, msg));
                }
                let name = ident.unwrap().to_string().to_lowercase();
                let msg = match name.as_str() {
                    "threaded_scheduler" | "threaded" => {
                        format!("Set the runtime flavor with #[{}(flavor = \"threaded\")].", macro_name)
                    },
                    "basic_scheduler" | "current_thread" | "single_threaded" => {
                        format!("Set the runtime flavor with #[{}(flavor = \"current_thread\")].", macro_name)
                    },
                    "flavor" | "worker_threads" => {
                        format!("The `{}` attribute requires an argument.", name)
                    },
                    name => {
                        format!("Unknown attribute {} is specified; expected one of: `flavor`, `worker_threads`", name)
                    },
                };
                return Err(syn::Error::new_spanned(path, msg));
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "Unknown attribute inside the macro",
                ));
            }
        }
    }

    let config = config.build()?;

    let mut rt = match config.flavor {
        RuntimeFlavor::CurrentThread => quote! {
            tokio::runtime::Builder::new_current_thread()
        },
        RuntimeFlavor::Threaded => quote! {
            tokio::runtime::Builder::new_multi_thread()
        },
    };
    if let Some(v) = config.worker_threads {
        rt = quote! { #rt.worker_threads(#v) };
    }

    let header = {
        if is_test {
            quote! {
                #[::core::prelude::v1::test]
            }
        } else {
            quote! {}
        }
    };

    let result = quote! {
        #header
        #(#attrs)*
        #vis #sig {
            #rt
                .enable_all()
                .build()
                .unwrap()
                .block_on(async { #body })
        }
    };

    Ok(result.into())
}

#[cfg(not(test))] // Work around for rust-lang/rust#62127
pub(crate) fn main(args: TokenStream, item: TokenStream, rt_threaded: bool) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemFn);
    let args = syn::parse_macro_input!(args as syn::AttributeArgs);

    if input.sig.ident == "main" && !input.sig.inputs.is_empty() {
        let msg = "the main function cannot accept arguments";
        return syn::Error::new_spanned(&input.sig.ident, msg)
            .to_compile_error()
            .into();
    }

    parse_knobs(input, args, false, rt_threaded).unwrap_or_else(|e| e.to_compile_error().into())
}

pub(crate) fn test(args: TokenStream, item: TokenStream, rt_threaded: bool) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemFn);
    let args = syn::parse_macro_input!(args as syn::AttributeArgs);

    for attr in &input.attrs {
        if attr.path.is_ident("test") {
            let msg = "second test attribute is supplied";
            return syn::Error::new_spanned(&attr, msg)
                .to_compile_error()
                .into();
        }
    }

    if !input.sig.inputs.is_empty() {
        let msg = "the test function cannot accept arguments";
        return syn::Error::new_spanned(&input.sig.inputs, msg)
            .to_compile_error()
            .into();
    }

    parse_knobs(input, args, true, rt_threaded).unwrap_or_else(|e| e.to_compile_error().into())
}
