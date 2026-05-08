use darling::{FromAttributes, FromMeta};
use darling::util::Override;
use proc_macro2::{Span, Ident};
use syn::DeriveInput;
use proc_macro::TokenStream;

use syn::parse_macro_input;
use quote::quote;

#[derive(FromAttributes, Debug)]
#[darling(attributes(message), allow_unknown_fields)]
struct MessageParameters {
    ctos: Option<Override<()>>,
    stoc: Option<Override<()>>,
    gm: Option<Override<()>>,
    srvpru: Option<Override<()>>,
    flag: Option<u8>,
    mod_name: Option<String>
}

pub fn ygopro_message(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match _ygopro_message(input) {
        Ok(stream) => stream.into(),
        Err(s) => {
            let error = syn::Error::new(Span::call_site(), s).to_compile_error();
            quote!(#error).into()
        }
    }
}

pub fn _ygopro_message(input: DeriveInput) -> Result<proc_macro2::TokenStream, String> {
    let struct_ident = input.ident;
    let attributes = input.attrs;
    let message_parameter = MessageParameters::from_attributes(&attributes).map_err(|err| format!("Cannot parse message paramter:\n{:?}", err))?;
    let direction = if message_parameter.ctos.is_some()   { "CTOS" }
                     else if message_parameter.stoc.is_some()   { "STOC" }
                     else if message_parameter.gm.is_some()     {  "GM"  }
                     else if message_parameter.srvpru.is_some() { "Other"}
                     else { return Err("Don't specify a direction.".to_string()); };
    let ident = Ident::from_string(direction).unwrap();
    let lower_ident = Ident::from_string(&direction.to_lowercase()).map_err(|err| format!("Illegal direction:\n{:?}", err))?;
    let mod_name = match message_parameter.mod_name {
        Some(name) => {
            let ident = Ident::from_string(&name).map_err(|err| format!("Illegal mod name identifier:\n{:?}", err))?;
            quote!(#ident)
        }
        None => quote!(crate)
    };

    let stream = if direction == "Other" {
        if let Some(flag) = message_parameter.flag {
            quote!{
                impl #mod_name::message::PureMessage for #struct_ident {}
                impl #mod_name::message::Message for #struct_ident {
                    fn message_type() -> #mod_name::message::all::MessageType {
                        #mod_name::message::all::MessageType::#ident("srvpru", #flag)
                    }
                }
            }
        }
        else {
            return Err("Don't offer a flag".to_string())
        }
    } else {
        quote!{
            impl #mod_name::message::PureMessage for #struct_ident {}
            impl #mod_name::message::Message for #struct_ident {
                fn message_type() -> #mod_name::message::all::MessageType {
                    #mod_name::message::all::MessageType::#ident(#mod_name::message::#lower_ident::MessageType::#struct_ident)
                }
            }
        }
    };
    Ok(stream)
}
