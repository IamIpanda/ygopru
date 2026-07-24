use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;
use syn::parse_macro_input;
use proc_macro::TokenStream as StdTokenStream;

fn dots_to_self_field(tokens: TokenStream, field_ident: &syn::Ident) -> TokenStream {
    let mut iter = tokens.clone().into_iter();
    match iter.next() {
        Some(proc_macro2::TokenTree::Punct(ref punct)) if punct.as_char() == '.' => {
            let rest: TokenStream = iter.collect();
            quote! { self.#field_ident . #rest }
        }
        _ => tokens,
    }
}

fn is_primitive(ty: &syn::Type) -> bool {
    let syn::Type::Path(syn::TypePath { path, .. }) = ty else { return false };
    let Some(seg) = path.segments.last() else { return false };
    matches!(seg.ident.to_string().as_str(),
        "u8" | "u16" | "u32" | "u64" | "u128"
        | "i8" | "i16" | "i32" | "i64" | "i128"
        | "f32" | "f64" | "bool" | "char" | "usize" | "isize")
}

pub fn mask(input: StdTokenStream) -> StdTokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_ident = input.ident;
    let generics = input.generics;

    let mut mask_statements: Vec<TokenStream> = vec![];
    let mut towards_unconditional: Vec<TokenStream> = vec![];
    let mut towards_conditional: Vec<TokenStream> = vec![];

    match input.data {
        syn::Data::Struct(syn::DataStruct { fields: syn::Fields::Named(fields), .. }) => {
            for field in &fields.named {
                let Some(ident) = &field.ident else { continue };

                let mask_attr = field.attrs.iter().find(|attr| attr.path.is_ident("mask"));
                let mask_if_attr = field.attrs.iter().find(|attr| attr.path.is_ident("mask_if"));

                let mask_action = match mask_attr {
                    None => quote! {},
                    Some(attr) if attr.tokens.is_empty() => {
                        if is_primitive(&field.ty) {
                            quote! { self.#ident = Default::default(); }
                        } else {
                            quote! { self.#ident.mask(); }
                        }
                    }
                    Some(attr) => {
                        let mut iter = attr.tokens.clone().into_iter();
                        match iter.next() {
                            Some(proc_macro2::TokenTree::Punct(ref punct)) if punct.as_char() == '.' => {
                                let rest: TokenStream = iter.collect();
                                quote! { self.#ident . #rest; }
                            }
                            _ => quote! { self.#ident = #attr.tokens; },
                        }
                    }
                };

                let is_auto_non_primitive = mask_attr.map_or(false, |a| a.tokens.is_empty()) && !is_primitive(&field.ty);

                if !mask_action.is_empty() {
                    mask_statements.push(mask_action.clone());
                    if mask_if_attr.is_none() {
                        let towards_action = if is_auto_non_primitive {
                            quote! { self.#ident.mask_towards(player); }
                        } else {
                            mask_action.clone()
                        };
                        towards_unconditional.push(towards_action);
                    }
                }

                if let Some(mask_if_attr) = mask_if_attr {
                    let condition = dots_to_self_field(mask_if_attr.tokens.clone(), ident);
                    let action = if mask_action.is_empty() {
                        let default = quote! { self.#ident = Default::default(); };
                        mask_statements.push(default.clone());
                        default
                    } else if is_auto_non_primitive {
                        quote! { self.#ident.mask_towards(player); }
                    } else {
                        mask_action.clone()
                    };
                    towards_conditional.push(quote! { if #condition { #action } });
                }
            }
        }
        _ => {}
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let mask_towards = if mask_statements.is_empty() {
        quote! {}
    } else {
        quote! {
            fn mask_towards(&mut self, player: CorePlayer) {
                #(#towards_unconditional)*
                #(#towards_conditional)*
            }
        }
    };

    let expanded = quote! {
        impl #impl_generics Mask for #struct_ident #ty_generics #where_clause {
            fn mask(&mut self) {
                #(#mask_statements)*
            }
            #mask_towards
        }
    };

    expanded.into()
}
