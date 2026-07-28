use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::ItemFn;
use syn::NestedMeta;
use syn::Path;
use syn::Ident;
use syn::Token;

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

pub struct RegisterInfo {
    #[allow(dead_code)]
    pub slice_expression: syn::Expr,
    pub handler_type: syn::Type,
    pub key_type: syn::Type,
}

pub fn parse_register_info(tokens: TokenStream2) -> syn::Result<RegisterInfo> {
    syn::parse::Parser::parse2(
        |input: syn::parse::ParseStream| {
            let slice_expression = input.parse::<syn::Expr>()?;

            let handler_type = if input.peek(Token![as]) {
                input.parse::<Token![as]>()?;
                input.parse::<syn::Type>()?
            } else {
                syn::parse_str::<syn::Type>("Handler")?
            };

            let key_type = if input.fork().parse::<syn::Ident>().map_or(false, |ident| ident == "with") {
                input.parse::<syn::Ident>()?;
                input.parse::<syn::Type>()?
            } else {
                syn::parse_str::<syn::Type>("u8")?
            };

            Ok(RegisterInfo { slice_expression, handler_type, key_type })
        },
        tokens,
    )
}

fn extract_type_suffix(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(type_path) => {
            type_path.path.segments.last()
                .map(|seg| seg.ident.to_string().to_lowercase())
                .unwrap_or_else(|| "handler".to_string())
        }
        _ => "handler".to_string(),
    }
}

pub fn shared_impl(args: ParsedArgs, function: ItemFn) -> TokenStream2 {
    let function_ident = &function.sig.ident;
    let function_name = function_ident.to_string();

    let priority = args.priority
        .map(|lit| quote! { #lit })
        .unwrap_or_else(|| quote! { 128 });

    let key = args.key;

    let mut register_infos = Vec::new();
    for attr in &function.attrs {
        if attr.path.is_ident("register_to") {
            if let Ok(info) = parse_register_info(attr.tokens.clone()) {
                register_infos.push(info);
            }
        }
    }

    if register_infos.is_empty() {
        let error = syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("no #[register_to] attribute found on handler `{function_name}`"),
        ).to_compile_error();
        return quote! {
            #error
            #function
        };
    }

    let registrations: Vec<_> = register_infos.iter().map(|info| {
        let suffix = extract_type_suffix(&info.handler_type);
        let builder_ident = Ident::new(
            &format!("build_handle_{function_name}_{suffix}"),
            function_ident.span(),
        );
        let key_type = &info.key_type;
        let handler_type = &info.handler_type;

        quote! {
            fn #builder_ident() -> (#key_type, #handler_type) {
                (
                    ::std::convert::Into::<#key_type>::into(<#key as ::ygopro_data::message::Message>::message_type()),
                    #handler_type::new(#priority, #function_name, module_path!(), #function_ident),
                )
            }
        }
    }).collect();

    quote! {
        #function
        #(#registrations)*
    }
}

pub fn register_to_impl(info: RegisterInfo, function: ItemFn) -> TokenStream2 {
    let function_ident = &function.sig.ident;
    let function_name = function_ident.to_string();

    let suffix = extract_type_suffix(&info.handler_type);

    let builder_ident = Ident::new(
        &format!("build_handle_{function_name}_{suffix}"),
        function_ident.span(),
    );

    let register_ident = Ident::new(
        &format!("REGISTER_{}_{}", function_ident.to_string().to_uppercase(), suffix.to_uppercase()),
        function_ident.span(),
    );

    let slice_expression = &info.slice_expression;
    let key_type = &info.key_type;
    let handler_type = &info.handler_type;

    quote! {
        #function

        #[linkme::distributed_slice(#slice_expression)]
        static #register_ident: fn() -> (#key_type, #handler_type) = #builder_ident;
    }
}
