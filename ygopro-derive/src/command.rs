//! `#[command]` attribute: registers a command handler into a
//! `COMMANDS`-style distributed slice. The command name is the function name
//! unless the attribute carries one.

use darling::FromMeta;
use proc_macro2::TokenStream as TokenStream2;
use proc_macro2::Ident;
use quote::quote;
use syn::AttributeArgs;
use syn::ItemFn;
use syn::LitStr;

use crate::registry;

#[derive(FromMeta)]
struct CommandArgs {
    #[darling(default)]
    name: Option<LitStr>,
    #[darling(default)]
    module: Option<LitStr>,
}

pub fn command_impl(attr: AttributeArgs, function: ItemFn) -> TokenStream2 {
    let function_ident = &function.sig.ident;
    let function_name = function_ident.to_string();

    let args = match CommandArgs::from_list(&attr) {
        Ok(args) => args,
        Err(err) => return err.write_errors().into(),
    };
    let name = match &args.name {
        Some(name) => quote! { #name },
        None => quote! { #function_name },
    };
    let module = match &args.module {
        Some(module) => quote! { #module },
        None => quote! { module_path!() },
    };

    let register_infos = registry::parse_registers(&function);
    if register_infos.is_empty() {
        let error = syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("no #[register_to] attribute found on command `{function_name}`"),
        ).to_compile_error();
        return quote! {
            #error
            #function
        };
    }

    let registrations: Vec<_> = register_infos.iter().map(|info| {
        let suffix = registry::extract_type_suffix(&info.handler_type);
        let builder_ident = Ident::new(
            &format!("build_handle_{function_name}_{suffix}"),
            function_ident.span(),
        );
        let key_type = &info.key_type;
        let handler_type = &info.handler_type;

        quote! {
            fn #builder_ident() -> (#key_type, #handler_type) {
                (#name, #handler_type::new(128, #name, #module, #function_ident))
            }
        }
    }).collect();

    quote! {
        #function
        #(#registrations)*
    }
}
