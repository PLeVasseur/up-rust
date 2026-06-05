/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::{parse_macro_input, spanned::Spanned, Data, DeriveInput, Fields, Lit, Type};

struct StablePayloadArgs {
    type_name: String,
}

#[proc_macro_derive(StablePayload, attributes(stable_payload))]
pub fn derive_stable_payload(input: TokenStream) -> TokenStream {
    expand_stable_payload(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_stable_payload(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let args = parse_args(&input)?;
    ensure_repr_c_or_transparent(&input)?;

    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "StablePayload derive does not support generic types yet",
        ));
    }

    let name = &input.ident;
    let field_checks = stable_field_checks(&input)?;
    let type_name = args.type_name;

    Ok(quote! {
        const _: () = {
            assert!(
                !::core::mem::needs_drop::<#name>(),
                "StablePayload types must not implement Drop or contain fields that need drop"
            );
        };

        // SAFETY:
        // - The derive requires `#[repr(C)]` or `#[repr(transparent)]`, rejects
        //   process-local fields, and rejects types that need drop glue.
        // - Field checks recursively require every field to satisfy up-rust's
        //   stable payload field proof.
        // - The user-provided `type_name` is the stable identity for this type.
        unsafe impl ::up_rust::payload::StablePayload for #name {
            const TYPE_NAME: &'static str = #type_name;

            fn __stable_payload_field_check(&self) {
                #field_checks
            }
        }

        // SAFETY: The generated `StablePayload` impl above establishes the field
        // proof required when this type is nested in another stable payload.
        unsafe impl ::up_rust::payload::StablePayloadField for #name {}
    })
}

fn stable_field_checks(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => {
                let checks = fields
                    .named
                    .iter()
                    .map(|field| {
                        let ident = field.ident.as_ref().expect("named fields have identifiers");
                        validate_stable_field_type(&field.ty)?;
                        let ty = &field.ty;
                        Ok(quote_spanned! {ty.span()=>
                            <#ty as ::up_rust::payload::StablePayloadField>::__stable_payload_field_check(&self.#ident);
                        })
                    })
                    .collect::<syn::Result<Vec<_>>>()?;
                Ok(quote! { #(#checks)* })
            }
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let checks = fields
                    .unnamed
                    .iter()
                    .enumerate()
                    .map(|(index, field)| {
                        validate_stable_field_type(&field.ty)?;
                        let index = syn::Index::from(index);
                        let ty = &field.ty;
                        Ok(quote_spanned! {ty.span()=>
                            <#ty as ::up_rust::payload::StablePayloadField>::__stable_payload_field_check(&self.#index);
                        })
                    })
                    .collect::<syn::Result<Vec<_>>>()?;
                Ok(quote! { #(#checks)* })
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
            Fields::Unit => Ok(quote! {}),
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
