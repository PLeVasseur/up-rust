/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::{parse_macro_input, spanned::Spanned, Data, DeriveInput, Fields, Lit, LitStr, Type};

#[proc_macro_derive(StablePayload, attributes(stable_payload))]
pub fn derive_stable_payload(input: TokenStream) -> TokenStream {
    expand_stable_payload(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(ByteBackedStablePayload, attributes(stable_payload))]
pub fn derive_byte_backed_stable_payload(input: TokenStream) -> TokenStream {
    expand_byte_backed_stable_payload(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

struct StablePayloadArgs {
    type_name: String,
}

fn expand_stable_payload(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let args = parse_args(&input)?;
    let name = &input.ident;
    let shape = analyze_stable_payload_shape(&input)?;
    let field_checks = shape.field_checks;
    let field_size_sum = shape.field_size_sum;
    let field_byte_backed_terms = shape.field_byte_backed_terms;

    let type_name = args.type_name;

    Ok(quote! {
        const _: () = {
            assert!(
                !::core::mem::needs_drop::<#name>(),
                "StablePayload types must not implement Drop or contain fields that need drop"
            );
        };

        unsafe impl ::up_rust::payload::ZeroCopySend for #name {
            unsafe fn type_name() -> &'static str {
                #type_name
            }

            fn __is_zero_copy_send(&self) {
                #field_checks
            }
        }

        unsafe impl ::up_rust::payload::StablePayload for #name {
            const SUPPORTS_BYTE_BACKED_UNINIT: bool =
                !::core::mem::needs_drop::<Self>()
                && ::core::mem::size_of::<Self>() == (#field_size_sum)
                && (#field_byte_backed_terms);

            fn stable_type_name() -> &'static str {
                #type_name
            }
        }
    })
}

fn expand_byte_backed_stable_payload(
    input: DeriveInput,
) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let shape = analyze_stable_payload_shape(&input)?;
    let field_size_sum = shape.field_size_sum;
    let byte_backed_field_checks = shape.byte_backed_field_checks;
    let field_offset_checks = shape.field_offset_checks;

    Ok(quote! {
        const _: () = {
            #field_offset_checks
            #byte_backed_field_checks
            assert!(
                !::core::mem::needs_drop::<#name>(),
                "ByteBackedStablePayload types must not implement Drop or contain fields that need drop"
            );
            assert!(
                ::core::mem::size_of::<#name>() == (#field_size_sum),
                "ByteBackedStablePayload types must not have implicit trailing padding; use explicit initialized padding fields"
            );
            assert!(
                <#name as ::up_rust::payload::StablePayload>::SUPPORTS_BYTE_BACKED_UNINIT,
                "ByteBackedStablePayload derive requires a matching StablePayload implementation with byte-backed support"
            );
        };

        unsafe impl ::up_rust::payload::ByteBackedStablePayload for #name {}
    })
}

struct StablePayloadShape {
    field_checks: proc_macro2::TokenStream,
    field_size_sum: proc_macro2::TokenStream,
    field_byte_backed_terms: proc_macro2::TokenStream,
    byte_backed_field_checks: proc_macro2::TokenStream,
    field_offset_checks: proc_macro2::TokenStream,
}

fn analyze_stable_payload_shape(input: &DeriveInput) -> syn::Result<StablePayloadShape> {
    ensure_repr_c_or_transparent(input)?;
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "StablePayload derive does not support generic types yet",
        ));
    }

    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => {
                let name = &input.ident;
                let checks = fields
                    .named
                    .iter()
                    .map(|field| {
                        let ident = field.ident.as_ref().expect("named fields have identifiers");
                        validate_stable_field_type(&field.ty)?;
                        Ok(quote! {
                            ::up_rust::payload::ZeroCopySend::__is_zero_copy_send(&self.#ident);
                        })
                    })
                    .collect::<syn::Result<Vec<_>>>()?;
                let sizes = fields.named.iter().map(|field| &field.ty);
                let byte_backed_fields = fields.named.iter().map(|field| &field.ty);
                let mut preceding_sizes = Vec::new();
                let mut field_offset_checks = Vec::new();
                let mut byte_backed_field_checks = Vec::new();
                for field in &fields.named {
                    let ident = field.ident.as_ref().expect("named fields have identifiers");
                    let ty = &field.ty;
                    let expected_offset = quote! { 0_usize #(+ ::core::mem::size_of::<#preceding_sizes>())* };
                    let padding_message = LitStr::new(
                        &format!(
                            "ByteBackedStablePayload field `{ident}` has implicit padding before it; add explicit initialized padding fields"
                        ),
                        ident.span(),
                    );
                    field_offset_checks.push(quote_spanned! {ident.span()=>
                        assert!(
                            ::core::mem::offset_of!(#name, #ident) == (#expected_offset),
                            #padding_message
                        );
                    });

                    let byte_backed_message = LitStr::new(
                        &format!(
                            "ByteBackedStablePayload field `{ident}` must be recursively byte-backed"
                        ),
                        ident.span(),
                    );
                    byte_backed_field_checks.push(quote_spanned! {ty.span()=>
                        assert!(
                            ::up_rust::__up_rust_byte_backed_stable_field_supported!(#ty),
                            #byte_backed_message
                        );
                    });
                    preceding_sizes.push(ty);
                }
                Ok(StablePayloadShape {
                    field_checks: quote! { #(#checks)* },
                    field_size_sum: quote! { 0_usize #(+ ::core::mem::size_of::<#sizes>())* },
                    field_byte_backed_terms: quote! { true #(&& ::up_rust::__up_rust_byte_backed_stable_field_supported!(#byte_backed_fields))* },
                    byte_backed_field_checks: quote! { #(#byte_backed_field_checks)* },
                    field_offset_checks: quote! { #(#field_offset_checks)* },
                })
            }
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let checks = fields
                    .unnamed
                    .iter()
                    .enumerate()
                    .map(|(index, field)| {
                        let index = syn::Index::from(index);
                        validate_stable_field_type(&field.ty)?;
                        Ok(quote! {
                            ::up_rust::payload::ZeroCopySend::__is_zero_copy_send(&self.#index);
                        })
                    })
                    .collect::<syn::Result<Vec<_>>>()?;
                let sizes = fields.unnamed.iter().map(|field| &field.ty);
                let byte_backed_fields = fields.unnamed.iter().map(|field| &field.ty);
                let byte_backed_field_checks = fields.unnamed.iter().enumerate().map(|(index, field)| {
                    let ty = &field.ty;
                    let byte_backed_message = LitStr::new(
                        &format!(
                            "ByteBackedStablePayload tuple field `{index}` must be recursively byte-backed"
                        ),
                        field.span(),
                    );
                    quote_spanned! {ty.span()=>
                        assert!(
                            ::up_rust::__up_rust_byte_backed_stable_field_supported!(#ty),
                            #byte_backed_message
                        );
                    }
                });
                Ok(StablePayloadShape {
                    field_checks: quote! { #(#checks)* },
                    field_size_sum: quote! { 0_usize #(+ ::core::mem::size_of::<#sizes>())* },
                    field_byte_backed_terms: quote! { true #(&& ::up_rust::__up_rust_byte_backed_stable_field_supported!(#byte_backed_fields))* },
                    byte_backed_field_checks: quote! { #(#byte_backed_field_checks)* },
                    field_offset_checks: quote! {},
                })
            }
            Fields::Unnamed(fields) => {
                for field in &fields.unnamed {
                    validate_stable_field_type(&field.ty)?;
                }
                Err(syn::Error::new_spanned(
                    &data.fields,
                    "StablePayload supports named structs or one-field repr(transparent) structs",
                ))
            }
            Fields::Unit => Ok(StablePayloadShape {
                field_checks: quote! {},
                field_size_sum: quote! { 0_usize },
                field_byte_backed_terms: quote! { true },
                byte_backed_field_checks: quote! {},
                field_offset_checks: quote! {},
            }),
        },
        Data::Enum(data) => Err(syn::Error::new_spanned(
            data.enum_token,
            "StablePayload derive does not support enums",
        )),
        Data::Union(data) => Err(syn::Error::new_spanned(
            data.union_token,
            "StablePayload derive does not support unions",
        )),
    }
}

fn parse_args(input: &DeriveInput) -> syn::Result<StablePayloadArgs> {
    let mut type_name = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("stable_payload") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("type_name") {
                type_name = Some(meta.value()?.parse::<Lit>()?);
                Ok(())
            } else {
                Err(meta.error("unsupported stable_payload attribute key; expected type_name"))
            }
        })?;
    }

    let type_name = match type_name {
        Some(Lit::Str(value)) => value.value(),
        Some(other) => {
            return Err(syn::Error::new_spanned(
                other,
                "stable_payload type_name must be a string literal",
            ));
        }
        None => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "StablePayload derive requires #[stable_payload(type_name = \"...\")]",
            ));
        }
    };
    Ok(StablePayloadArgs { type_name })
}

fn ensure_repr_c_or_transparent(input: &DeriveInput) -> syn::Result<()> {
    let mut has_supported_repr = false;
    for attr in &input.attrs {
        if !attr.path().is_ident("repr") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("packed") {
                return Err(meta.error("StablePayload does not support repr(packed)"));
            }
            if meta.path.is_ident("C") || meta.path.is_ident("transparent") {
                has_supported_repr = true;
            }
            Ok(())
        })?;
    }
    if !has_supported_repr {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "StablePayload requires #[repr(C)] or #[repr(transparent)]",
        ));
    }
    Ok(())
}

fn validate_stable_field_type(ty: &Type) -> syn::Result<()> {
    match ty {
        Type::Reference(value) => Err(syn::Error::new_spanned(
            value,
            "StablePayload fields must not be references",
        )),
        Type::Ptr(value) => Err(syn::Error::new_spanned(
            value,
            "StablePayload fields must not be raw pointers",
        )),
        Type::BareFn(value) => Err(syn::Error::new_spanned(
            value,
            "StablePayload fields must not be function pointers",
        )),
        Type::Path(value) if path_last_segment_is(value, "String") => Err(syn::Error::new_spanned(
            value,
            "StablePayload fields must not own heap allocations",
        )),
        Type::Path(value)
            if ["Vec", "Box", "Arc"]
                .into_iter()
                .any(|ident| path_last_segment_is(value, ident)) =>
        {
            Err(syn::Error::new_spanned(
                value,
                "StablePayload fields must not own heap allocations",
            ))
        }
        Type::Array(value) => validate_stable_field_type(&value.elem),
        _ => Ok(()),
    }
}

fn path_last_segment_is(value: &syn::TypePath, ident: &str) -> bool {
    value
        .path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == ident)
}
