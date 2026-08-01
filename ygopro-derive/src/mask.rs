use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;
use syn::parse_macro_input;
use proc_macro::TokenStream as StdTokenStream;

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
    let mut has_unconditional_mask = false;
    let mut mask_conditions: Vec<TokenStream> = vec![];
    let mut waiting_for_field: Option<syn::Ident> = None;

    match input.data {
        syn::Data::Struct(syn::DataStruct { fields: syn::Fields::Named(fields), .. }) => {
            for field in &fields.named {
                let Some(ident) = &field.ident else { continue };

                let mask_attr = field.attrs.iter().find(|attr| attr.path.is_ident("mask"));
                let mask_if_attr = field.attrs.iter().find(|attr| attr.path.is_ident("mask_if"));
                let wait_for_attr = field.attrs.iter().find(|attr| attr.path.is_ident("wait_for"));

                if wait_for_attr.is_some() && waiting_for_field.is_none() {
                    waiting_for_field = Some(ident.clone());
                }

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
                        let tokens: TokenStream = syn::parse2::<syn::Expr>(attr.tokens.clone())
                            .map(|expr| match expr {
                                syn::Expr::Paren(paren) => {
                                    let expr = paren.expr;
                                    quote! { #expr }
                                },
                                other => quote! { #other },
                            })
                            .unwrap_or_default();
                        quote! { self.#ident = #tokens; }
                    }
                };

                if !mask_action.is_empty() {
                    mask_statements.push(mask_action.clone());
                    if mask_if_attr.is_none() {
                        has_unconditional_mask = true;
                    }
                }

                if let Some(mask_if_attr) = mask_if_attr {
                    let condition: TokenStream = syn::parse2::<syn::Expr>(mask_if_attr.tokens.clone())
                        .map(|expr| match expr {
                            syn::Expr::Paren(paren) => {
                                let expr = paren.expr;
                                quote! { #expr }
                            },
                            other => quote! { #other },
                        })
                        .unwrap_or_default();
                    mask_conditions.push(condition);
                    if mask_action.is_empty() {
                        mask_statements.push(quote! { self.#ident = Default::default(); });
                    }
                }
            }
        }
        _ => {}
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let should_mask = if mask_statements.is_empty() {
        quote! {}
    } else if has_unconditional_mask {
        quote! {
            fn should_mask(&self, _player: CorePlayer) -> bool {
                true
            }
        }
    } else if !mask_conditions.is_empty() {
        quote! {
            fn should_mask(&self, player: CorePlayer) -> bool {
                #(#mask_conditions)||*
            }
        }
    } else {
        quote! {}
    };

    let waiting_for = if let Some(ident) = waiting_for_field {
        quote! {
            fn waiting_for(&self) -> Option<CorePlayer> {
                Some(self.#ident)
            }
        }
    } else {
        quote! {}
    };

    let expanded = quote! {
        impl #impl_generics GameMessage for #struct_ident #ty_generics #where_clause {
            fn mask(&mut self) {
                #(#mask_statements)*
            }
            #should_mask
            #waiting_for
        }
    };

    expanded.into()
}
