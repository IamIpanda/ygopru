use proc_macro::TokenStream;
use syn::parse_macro_input;
use syn::AttributeArgs;
use syn::ItemFn;

mod message;
mod registry;

fn dispatch(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as AttributeArgs);
    let function = parse_macro_input!(item as ItemFn);
    let args = match registry::parse_args(&attr) {
        Ok(args) => args,
        Err(err) => return err.to_compile_error().into(),
    };
    registry::shared_impl(args, function).into()
}

#[proc_macro_derive(Message, attributes(message))]
pub fn ygopro_message(input: TokenStream) -> TokenStream {
    message::ygopro_message(input)
}

#[proc_macro_attribute]
pub fn before(attr: TokenStream, item: TokenStream) -> TokenStream {
    dispatch(attr, item)
}

#[proc_macro_attribute]
pub fn after(attr: TokenStream, item: TokenStream) -> TokenStream {
    dispatch(attr, item)
}

#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    dispatch(attr, item)
}
