use darling::FromMeta;
use darling::util::Flag;
use proc_macro2::Ident;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Error;
use syn::ItemFn;
use syn::LitStr;
use syn::NestedMeta;

#[derive(FromMeta)]
struct Args {
    #[darling(default)]
    name: Option<LitStr>,
    #[darling(default)]
    global: Flag,
}

pub fn api_impl(attr: &[NestedMeta], function: ItemFn) -> syn::Result<TokenStream2> {
    let mut plugin = None;
    let mut named = Vec::new();
    for item in attr {
        match item {
            NestedMeta::Meta(syn::Meta::Path(path)) if !path.is_ident("global") && plugin.is_none() => plugin = Some(path.clone()),
            _ => named.push(item.clone()),
        }
    }
    let args = Args::from_list(&named)?;
    let function_ident = &function.sig.ident;
    let registration_ident = Ident::new(&function_ident.to_string().to_uppercase(), function_ident.span());
    let registration = if args.global.is_present() {
        if let Some(plugin) = plugin {
            return Err(Error::new_spanned(plugin, "`global` mounts unconditionally, so it takes no plugin name"));
        }
        quote! {
            #[::linkme::distributed_slice(crate::SRVPRO_API_GLOBAL_ROUTERS)]
            static #registration_ident: fn() -> ::axum::Router = #function_ident;
        }
    } else {
        let plugin = match args.name {
            Some(name) => quote! { #name },
            None => plugin.map_or_else(|| quote! { module_path!() }, |plugin| quote! { #plugin }),
        };
        quote! {
            #[::linkme::distributed_slice(crate::SRVPRO_API_ROUTERS)]
            static #registration_ident: (&'static str, fn() -> ::axum::Router) = (#plugin, #function_ident);
        }
    };
    Ok(quote! {
        #function
        #registration
    })
}
