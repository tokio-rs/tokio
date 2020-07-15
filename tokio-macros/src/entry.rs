use proc_macro::TokenStream;
use quote::quote;
use std::num::NonZeroUsize;

#[derive(Clone, Copy, PartialEq)]
enum Runtime {
    Basic,
    Threaded,
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

    let mut runtime = None;
    let mut core_threads = None;
    let mut max_threads = None;

    for arg in args {
        match arg {
            syn::NestedMeta::Meta(syn::Meta::NameValue(namevalue)) => {
                let ident = namevalue.path.get_ident();
                if ident.is_none() {
                    let msg = "Must have specified ident";
                    return Err(syn::Error::new_spanned(namevalue, msg));
                }
                match ident.unwrap().to_string().to_lowercase().as_str() {
                    "core_threads" => {
                        if rt_threaded {
                            match &namevalue.lit {
                                syn::Lit::Int(expr) => {
                                    let num = expr.base10_parse::<NonZeroUsize>().unwrap();
                                    if num.get() > 1 {
                                        runtime = Some(Runtime::Threaded);
                                    } else {
                                        runtime = Some(Runtime::Basic);
                                    }

                                    if let Some(v) = max_threads {
                                        if v < num {
                                            return Err(syn::Error::new_spanned(
                                                namevalue,
                                                "max_threads cannot be less than core_threads",
                                            ));
                                        }
                                    }

                                    core_threads = Some(num);
                                }
                                _ => {
                                    return Err(syn::Error::new_spanned(
                                        namevalue,
                                        "core_threads argument must be an int",
                                    ))
                                }
                            }
                        } else {
                            return Err(syn::Error::new_spanned(
                                namevalue,
                                "core_threads can only be set with rt-threaded feature flag enabled",
                            ));
                        }
                    }
                    "max_threads" => match &namevalue.lit {
                        syn::Lit::Int(expr) => {
                            let num = expr.base10_parse::<NonZeroUsize>().unwrap();

                            if let Some(v) = core_threads {
                                if num < v {
                                    return Err(syn::Error::new_spanned(
                                        namevalue,
                                        "max_threads cannot be less than core_threads",
                                    ));
                                }
                            }
                            max_threads = Some(num);
                        }
                        _ => {
                            return Err(syn::Error::new_spanned(
                                namevalue,
                                "max_threads argument must be an int",
                            ))
                        }
                    },
                    name => {
                        let msg = format!("Unknown attribute pair {} is specified; expected one of: `core_threads`, `max_threads`", name);
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
                match ident.unwrap().to_string().to_lowercase().as_str() {
                    "threaded_scheduler" => {
                        runtime = Some(runtime.unwrap_or_else(|| Runtime::Threaded))
                    }
                    "basic_scheduler" => runtime = Some(runtime.unwrap_or_else(|| Runtime::Basic)),
                    name => {
                        let msg = format!("Unknown attribute {} is specified; expected `basic_scheduler` or `threaded_scheduler`", name);
                        return Err(syn::Error::new_spanned(path, msg));
                    }
                }
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "Unknown attribute inside the macro",
                ));
            }
        }
    }

    let mut rt = quote! { tokio::runtime::Builder::new().basic_scheduler() };
    if rt_threaded && (runtime == Some(Runtime::Threaded) || (runtime.is_none() && !is_test)) {
        rt = quote! { #rt.threaded_scheduler() };
    }
    if let Some(v) = core_threads.map(|v| v.get()) {
        rt = quote! { #rt.core_threads(#v) };
    }
    if let Some(v) = max_threads.map(|v| v.get()) {
        rt = quote! { #rt.max_threads(#v) };
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
        return syn::Error::new_spanned(&input.sig.inputs, msg)
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

pub(crate) mod old {
    use proc_macro::TokenStream;
    use quote::quote;

    enum Runtime {
        Basic,
        Threaded,
        Auto,
    }

    #[cfg(not(test))] // Work around for rust-lang/rust#62127
    pub(crate) fn main(args: TokenStream, item: TokenStream) -> TokenStream {
        let mut input = syn::parse_macro_input!(item as syn::ItemFn);
        let args = syn::parse_macro_input!(args as syn::AttributeArgs);

        let sig = &mut input.sig;
        let name = &sig.ident;
        let inputs = &sig.inputs;
        let body = &input.block;
        let attrs = &input.attrs;
        let vis = input.vis;

        if sig.asyncness.is_none() {
            let msg = "the async keyword is missing from the function declaration";
            return syn::Error::new_spanned(sig.fn_token, msg)
                .to_compile_error()
                .into();
        } else if name == "main" && !inputs.is_empty() {
            let msg = "the main function cannot accept arguments";
            return syn::Error::new_spanned(&sig.inputs, msg)
                .to_compile_error()
                .into();
        }

        sig.asyncness = None;

        let mut runtime = Runtime::Auto;

        for arg in args {
            if let syn::NestedMeta::Meta(syn::Meta::Path(path)) = arg {
                let ident = path.get_ident();
                if ident.is_none() {
                    let msg = "Must have specified ident";
                    return syn::Error::new_spanned(path, msg).to_compile_error().into();
                }
                match ident.unwrap().to_string().to_lowercase().as_str() {
                    "threaded_scheduler" => runtime = Runtime::Threaded,
                    "basic_scheduler" => runtime = Runtime::Basic,
                    name => {
                        let msg = format!("Unknown attribute {} is specified; expected `basic_scheduler` or `threaded_scheduler`", name);
                        return syn::Error::new_spanned(path, msg).to_compile_error().into();
                    }
                }
            }
        }

        let result = match runtime {
            Runtime::Threaded | Runtime::Auto => quote! {
                #(#attrs)*
                #vis #sig {
                    tokio::runtime::Runtime::new().unwrap().block_on(async { #body })
                }
            },
            Runtime::Basic => quote! {
                #(#attrs)*
                #vis #sig {
                    tokio::runtime::Builder::new()
                        .basic_scheduler()
                        .enable_all()
                        .build()
                        .unwrap()
                        .block_on(async { #body })
                }
            },
        };

        result.into()
    }

    pub(crate) fn test(args: TokenStream, item: TokenStream) -> TokenStream {
        let input = syn::parse_macro_input!(item as syn::ItemFn);
        let args = syn::parse_macro_input!(args as syn::AttributeArgs);

        let ret = &input.sig.output;
        let name = &input.sig.ident;
        let body = &input.block;
        let attrs = &input.attrs;
        let vis = input.vis;

        for attr in attrs {
            if attr.path.is_ident("test") {
                let msg = "second test attribute is supplied";
                return syn::Error::new_spanned(&attr, msg)
                    .to_compile_error()
                    .into();
            }
        }

        if input.sig.asyncness.is_none() {
            let msg = "the async keyword is missing from the function declaration";
            return syn::Error::new_spanned(&input.sig.fn_token, msg)
                .to_compile_error()
                .into();
        } else if !input.sig.inputs.is_empty() {
            let msg = "the test function cannot accept arguments";
            return syn::Error::new_spanned(&input.sig.inputs, msg)
                .to_compile_error()
                .into();
        }

        let mut runtime = Runtime::Auto;

        for arg in args {
            if let syn::NestedMeta::Meta(syn::Meta::Path(path)) = arg {
                let ident = path.get_ident();
                if ident.is_none() {
                    let msg = "Must have specified ident";
                    return syn::Error::new_spanned(path, msg).to_compile_error().into();
                }
                match ident.unwrap().to_string().to_lowercase().as_str() {
                    "threaded_scheduler" => runtime = Runtime::Threaded,
                    "basic_scheduler" => runtime = Runtime::Basic,
                    name => {
                        let msg = format!("Unknown attribute {} is specified; expected `basic_scheduler` or `threaded_scheduler`", name);
                        return syn::Error::new_spanned(path, msg).to_compile_error().into();
                    }
                }
            }
        }

        let result = match runtime {
            Runtime::Threaded => quote! {
                #[::core::prelude::v1::test]
                #(#attrs)*
                #vis fn #name() #ret {
                    tokio::runtime::Runtime::new().unwrap().block_on(async { #body })
                }
            },
            Runtime::Basic | Runtime::Auto => quote! {
                #[::core::prelude::v1::test]
                #(#attrs)*
                #vis fn #name() #ret {
                    tokio::runtime::Builder::new()
                        .basic_scheduler()
                        .enable_all()
                        .build()
                        .unwrap()
                        .block_on(async { #body })
                }
            },
        };

        result.into()
    }
}
