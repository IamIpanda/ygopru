use proc_macro::TokenStream;
use syn::parse;
use syn::parse_macro_input;
use syn::AttributeArgs;
use syn::Ident;
use syn::ItemFn;
use syn::Token;

mod mask;
mod message;
mod registry;

#[proc_macro_derive(Mask, attributes(mask, mask_if))]
pub fn mask(input: TokenStream) -> TokenStream {
    mask::mask(input)
}

#[proc_macro_derive(Message, attributes(message))]
pub fn ygopro_message(input: TokenStream) -> TokenStream {
    message::ygopro_message(input)
}

#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    dispatch(attr, item)
}

#[proc_macro_attribute]
pub fn before(attr: TokenStream, item: TokenStream) -> TokenStream {
    dispatch(attr, item)
}

#[proc_macro_attribute]
pub fn after(attr: TokenStream, item: TokenStream) -> TokenStream {
    dispatch(attr, item)
}

fn dispatch(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as AttributeArgs);
    let function = parse_macro_input!(item as ItemFn);
    let args = match registry::parse_args(&attr) {
        Ok(args) => args,
        Err(err) => return err.to_compile_error().into(),
    };
    registry::shared_impl(args, function).into()
}

#[proc_macro_attribute]
pub fn register_to(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2: proc_macro2::TokenStream = attr.into();
    let slice_idents = parse::Parser::parse2(
        |input: syn::parse::ParseStream| {
            let mut idents = Vec::new();
            while !input.is_empty() {
                idents.push(input.parse::<Ident>()?);
                if input.is_empty() { break; }
                input.parse::<Token![,]>()?;
            }
            Ok(idents)
        },
        attr2,
    );
    let slice_idents = match slice_idents {
        Ok(v) => v,
        Err(err) => return err.to_compile_error().into(),
    };
    let function = parse_macro_input!(item as ItemFn);
    registry::register_to_impl(slice_idents, function).into()
}
