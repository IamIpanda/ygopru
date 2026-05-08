use proc_macro::TokenStream;

mod message;

#[proc_macro_derive(Message, attributes(message))]
pub fn ygopro_message(input: TokenStream) -> TokenStream {
    message::ygopro_message(input)
}
