use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::ItemFn;
use syn::NestedMeta;
use syn::Path;

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

pub fn shared_impl(_args: ParsedArgs, function: ItemFn) -> TokenStream2 {
    quote! { #function }
}
