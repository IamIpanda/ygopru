use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::ItemFn;
use syn::NestedMeta;
use syn::Path;
use syn::Ident;

pub struct ParsedArgs {
    #[allow(dead_code)]
    pub key: Path,
    #[allow(dead_code)]
    pub priority: Option<syn::Lit>,
}

pub fn parse_args(attr: &[NestedMeta]) -> Result<ParsedArgs, syn::Error> {
    let mut key: Option<Path> = None;
    let mut priority: Option<syn::Lit> = None;

    for item in attr {
        match item {
            NestedMeta::Lit(lit) => {
                priority = Some(lit.clone());
            }
            NestedMeta::Meta(meta) => match meta {
                syn::Meta::Path(path) => {
                    if key.is_some() {
                        return Err(syn::Error::new_spanned(path, "duplicate key"));
                    }
                    key = Some(path.clone());
                }
                syn::Meta::NameValue(name_value) => {
                    if name_value.path.is_ident("message") {
                        if let syn::Lit::Str(lit_str) = &name_value.lit {
                            let path: Path = lit_str.parse().map_err(|e| {
                                syn::Error::new_spanned(&name_value.lit, format!("invalid path: {}", e))
                            })?;
                            if key.is_some() {
                                return Err(syn::Error::new_spanned(&name_value, "duplicate key"));
                            }
                            key = Some(path);
                        } else {
                            return Err(syn::Error::new_spanned(&name_value, "message must be a string"));
                        }
                    } else if name_value.path.is_ident("priority") {
                        priority = Some(name_value.lit.clone());
                    } else {
                        return Err(syn::Error::new_spanned(&name_value.path, "unknown attribute"));
                    }
                }
                syn::Meta::List(list) => {
                    return Err(syn::Error::new_spanned(list, "unexpected list"));
                }
            },
        }
    }

    let key = key.ok_or_else(|| {
        syn::Error::new(proc_macro2::Span::call_site(), "missing key or message")
    })?;

    Ok(ParsedArgs { key, priority })
}

pub fn shared_impl(args: ParsedArgs, function: ItemFn) -> TokenStream2 {
    let function_ident = &function.sig.ident;
    let function_name = function_ident.to_string();

    let builder_ident = Ident::new(
        &format!("build_handle_{function_name}"),
        function_ident.span(),
    );

    let priority = args.priority
        .map(|lit| quote! { #lit })
        .unwrap_or_else(|| quote! { 128 });

    let key = args.key;

    quote! { 
        #function 

        fn #builder_ident() -> (HandlerKey, Handler) {
            (
                ::std::convert::Into::<HandlerKey>::into(<#key as ::ygopro_data::message::Message>::message_type()),
                Handler::new(#priority, #function_name, module_path!(), #function_ident),
            )
        }
    }
}

pub fn register_to_impl(slice_idents: Vec<Ident>, function: ItemFn) -> TokenStream2 {
    let function_ident = &function.sig.ident;
    let function_name = function_ident.to_string();

    let builder_ident = Ident::new(
        &format!("build_handle_{function_name}"),
        function_ident.span(),
    );

    let register_statics: Vec<_> = slice_idents.iter().enumerate().map(|(index, slice_ident)| {
        let register_ident = Ident::new(
            &format!("REGISTER_{}_{}", function_ident.to_string().to_uppercase(), index),
            function_ident.span(),
        );
        quote! {
            #[linkme::distributed_slice(#slice_ident)]
            static #register_ident: fn() -> (HandlerKey, Handler) = #builder_ident;
        }
    }).collect();

    quote! {
        #function
        #(#register_statics)*
    }
}
