/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

//! Derive macros for SDK stable native layouts and checked initialization.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, LitInt, LitStr, Type};

/// Derives a checked stable native layout with an explicit deployment name.
///
/// `#[stable_payload(type_name = "...", revision = N)]` sets the descriptor
/// revision (default 0); a changed layout under an existing name needs a new
/// revision, which is a new identity. `inventory` additionally registers a
/// concrete type for target-side export when the SDK's `native-inventory` feature
/// is enabled. Register generic instantiations explicitly with `register_native_type!`.
/// A consumer that renames the SDK dependency selects it with `crate = "path"`,
/// for example `#[stable_payload(type_name = "...", crate = "sdk")]`; both
/// derives then emit that path instead of `::up_rust`.
#[proc_macro_derive(StablePayload, attributes(stable_payload))]
pub fn derive_stable_payload(input: TokenStream) -> TokenStream {
    derive_stable_payload_impl(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives a typed initializer that tracks initialization of every payload field.
///
/// Named fields use their names; tuple fields use `field_0`, `field_1`, and so on.
/// Array fields provide borrowed-input and element-fill methods rather than
/// accepting a whole array by value. Generic, non-struct and empty inputs reject.
/// `<field>_fill_pattern` repeats a borrowed stable pattern with bulk in-place
/// writes; it rejects an empty pattern for a nonempty field and invokes no Clone.
#[proc_macro_derive(StablePayloadInit, attributes(stable_payload))]
pub fn derive_stable_payload_init(input: TokenStream) -> TokenStream {
    derive_stable_payload_init_impl(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn derive_stable_payload_init_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "StablePayloadInit does not support generic payload structs",
        ));
    }
    let fields = match &input.data {
        Data::Struct(data) => &data.fields,
        Data::Enum(_) | Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                name,
                "StablePayloadInit can only be derived for structs",
            ));
        }
    };
    if fields.is_empty() {
        return Err(syn::Error::new_spanned(
            name,
            "StablePayloadInit requires at least one field",
        ));
    }
    let krate = sdk_path(&input.attrs)?;

    let builder = format_ident!("__UpRustStablePayloadInitFor{name}");
    let states: Vec<_> = (0..fields.len())
        .map(|index| format_ident!("__UpRustField{index}"))
        .collect();
    let unset = quote!(#krate::payload::stable::StablePayloadInitUnset);
    let set = quote!(#krate::payload::stable::StablePayloadInitSet);
    let initial_states = states.iter().map(|_| unset.clone());
    let final_states = states.iter().map(|_| set.clone());

    let mut setters = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let method = field
            .ident
            .clone()
            .unwrap_or_else(|| format_ident!("field_{index}"));
        let member = field
            .ident
            .as_ref()
            .map(|ident| quote!(#ident))
            .unwrap_or_else(|| {
                let index = syn::Index::from(index);
                quote!(#index)
            });
        let ty = &field.ty;
        let impl_states: Vec<_> = states
            .iter()
            .enumerate()
            .map(|(state_index, state)| {
                if state_index == index {
                    unset.clone()
                } else {
                    quote!(#state)
                }
            })
            .collect();
        let output_states: Vec<_> = states
            .iter()
            .enumerate()
            .map(|(state_index, state)| {
                if state_index == index {
                    set.clone()
                } else {
                    quote!(#state)
                }
            })
            .collect();
        let other_states: Vec<_> = states
            .iter()
            .enumerate()
            .filter_map(|(state_index, state)| (state_index != index).then_some(state))
            .collect();

        let body = if let Type::Array(array) = ty {
            let elem = &array.elem;
            let len = &array.len;
            let from_array = format_ident!("{method}_from_array");
            let from_slice = format_ident!("{method}_from_slice");
            let fill = format_ident!("{method}_fill");
            let fill_with = format_ident!("{method}_fill_with");
            let fill_pattern = format_ident!("{method}_fill_pattern");
            quote! {
                pub fn #from_array(
                    self,
                    values: &[#elem; #len],
                ) -> #builder<'__up_rust_payload, #(#output_states),*>
                where
                    #elem: Clone,
                {
                    // SAFETY: Prepared storage is exclusive and aligned; this
                    // unset field's compiler-derived offset and array length
                    // identify exactly its elements, each written once.
                    unsafe {
                        #krate::payload::stable::write_stable_payload_array::<#name, #elem, #len, _>(
                            self.payload,
                            ::core::mem::offset_of!(#name, #member),
                            |index| values.get(index).expect("array initializer index").clone(),
                        );
                    }
                    #builder { payload: self.payload, marker: ::core::marker::PhantomData }
                }

                pub fn #from_slice(
                    self,
                    values: &[#elem],
                ) -> Result<#builder<'__up_rust_payload, #(#output_states),*>, #krate::UWireError>
                where
                    #elem: Clone,
                {
                    if values.len() != #len {
                        return Err(#krate::UWireError::invalid_payload_length(#len, values.len()));
                    }
                    Ok(self.#from_array(values.try_into().expect("checked array length")))
                }

                pub fn #fill(
                    self,
                    value: #elem,
                ) -> #builder<'__up_rust_payload, #(#output_states),*>
                where
                    #elem: Clone,
                {
                    self.#fill_with(|_| value.clone())
                }

                pub fn #fill_with<F>(
                    self,
                    element: F,
                ) -> #builder<'__up_rust_payload, #(#output_states),*>
                where
                    F: FnMut(usize) -> #elem,
                {
                    // SAFETY: The unset typestate prevents repeat writes; the
                    // compiler-derived array extent is within prepared storage.
                    // A panic never produces the all-set completion witness.
                    unsafe {
                        #krate::payload::stable::write_stable_payload_array::<#name, #elem, #len, F>(
                            self.payload,
                            ::core::mem::offset_of!(#name, #member),
                            element,
                        );
                    }
                    #builder { payload: self.payload, marker: ::core::marker::PhantomData }
                }

                /// Repeats a borrowed stable plain-data pattern into this array.
                ///
                /// No payload-sized temporary is built and Clone is not called.
                ///
                /// # Errors
                /// Rejects an empty pattern when the array field is nonempty.
                pub fn #fill_pattern(
                    self,
                    pattern: &[#elem],
                ) -> Result<#builder<'__up_rust_payload, #(#output_states),*>, #krate::StablePayloadPatternError> {
                    if #len != 0 && pattern.is_empty() {
                        return Err(#krate::StablePayloadPatternError::EmptyPattern);
                    }
                    // SAFETY: the generated offset/length identify this unset
                    // field in exclusive prepared storage. Safe callers cannot
                    // alias its uninitialized field through the borrowed pattern.
                    // The stable field contract permits plain-data duplication.
                    unsafe {
                        #krate::payload::stable::write_stable_payload_array_pattern::<#name, #elem, #len>(
                            self.payload, ::core::mem::offset_of!(#name, #member), pattern,
                        );
                    }
                    Ok(#builder { payload: self.payload, marker: ::core::marker::PhantomData })
                }
            }
        } else {
            quote! {
                pub fn #method(
                    self,
                    value: #ty,
                ) -> #builder<'__up_rust_payload, #(#output_states),*> {
                    // SAFETY: Exclusive prepared storage and the compiler's
                    // offset identify an aligned in-bounds field of this type;
                    // the unset state permits exactly one initialization.
                    unsafe {
                        #krate::payload::stable::write_stable_payload_field::<#name, #ty>(
                            self.payload,
                            ::core::mem::offset_of!(#name, #member),
                            value,
                        );
                    }
                    #builder { payload: self.payload, marker: ::core::marker::PhantomData }
                }
            }
        };
        setters.push(quote! {
            impl<'__up_rust_payload, #(#other_states),*>
                #builder<'__up_rust_payload, #(#impl_states),*>
            {
                #body
            }
        });
    }

    Ok(quote! {
        #[doc(hidden)]
        pub struct #builder<'__up_rust_payload, #(#states),*> {
            payload: *mut #name,
            marker: ::core::marker::PhantomData<(&'__up_rust_payload mut #name, #(#states),*)>,
        }

        #(#setters)*

        impl<'__up_rust_payload>
            #builder<'__up_rust_payload, #(#final_states),*>
        {
            pub fn finish(self) -> #krate::payload::stable::InitializedStablePayload<'__up_rust_payload, #name> {
                // SAFETY: Every field is in the set state and was written once;
                // the builder retains exclusive prepared storage for its lifetime.
                unsafe { #krate::payload::stable::finish_stable_payload_init(self.payload) }
            }
        }

        impl #krate::payload::stable::StablePayloadInit for #name {
            type Initializer<'__up_rust_payload> =
                #builder<'__up_rust_payload, #(#initial_states),*>;

            fn init(
                storage: &mut [::core::mem::MaybeUninit<u8>],
            ) -> Result<Self::Initializer<'_>, #krate::UWireError> {
                let payload = #krate::payload::stable::prepare_stable_payload_storage::<Self>(storage)?;
                Ok(#builder { payload, marker: ::core::marker::PhantomData })
            }
        }
    })
}

fn derive_stable_payload_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let mut has_stable_repr = false;
    for attr in &input.attrs {
        if attr.path().is_ident("repr") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("C") || meta.path.is_ident("transparent") {
                    has_stable_repr = true;
                } else if meta.path.is_ident("packed") {
                    return Err(meta.error("StablePayload does not support packed fields"));
                } else if meta.path.is_ident("align") {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    let _: LitInt = content.parse()?;
                }
                Ok(())
            })?;
        }
    }
    if !has_stable_repr {
        return Err(syn::Error::new_spanned(
            name,
            "StablePayload requires #[repr(C)] or #[repr(transparent)]",
        ));
    }

    let mut type_name = None::<LitStr>;
    let mut revision = None::<LitInt>;
    let mut inventory = false;
    for attr in &input.attrs {
        if !attr.path().is_ident("stable_payload") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("type_name") {
                type_name = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("revision") {
                revision = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("inventory") {
                inventory = true;
            } else if meta.path.is_ident("crate") {
                let _: LitStr = meta.value()?.parse()?;
            } else if meta.path.is_ident("payload_encoding_id") {
                return Err(meta.error("encoding IDs belong to NativeProfile, not a payload type"));
            } else {
                return Err(meta.error("unsupported stable_payload option"));
            }
            Ok(())
        })?;
    }
    let krate = sdk_path(&input.attrs)?;
    let type_name = type_name.ok_or_else(|| {
        syn::Error::new_spanned(
            name,
            "StablePayload requires #[stable_payload(type_name = \"...\")]",
        )
    })?;
    let revision = revision
        .map(|value| quote!(#value))
        .unwrap_or_else(|| quote!(0_u32));

    let fields = match &input.data {
        Data::Struct(data) => &data.fields,
        Data::Enum(_) | Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                name,
                "StablePayload can only be derived for structs",
            ));
        }
    };

    let mut generics = input.generics.clone();
    let mut validators = Vec::new();
    let mut always_valid = Vec::new();
    let mut representations = Vec::new();
    let mut field_sizes = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let ty = &field.ty;
        field_sizes.push(quote!(::core::mem::size_of::<#ty>()));
        generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#ty: #krate::payload::stable::StablePayloadField));
        always_valid.push(quote!(
            <#ty as #krate::payload::stable::StablePayloadField>::FIELD_BITS_ALWAYS_VALID
        ));
        let member = field
            .ident
            .as_ref()
            .map(|ident| quote!(#ident))
            .unwrap_or_else(|| {
                let index = syn::Index::from(index);
                quote!(#index)
            });
        let local = format_ident!("field_bytes_{index}");
        let field_name = field
            .ident
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| index.to_string());
        representations.push(quote! {
            #krate::NativeField::new(
                #field_name, ::core::mem::offset_of!(Self, #member) as u64,
                <#ty as #krate::payload::stable::StablePayloadField>::field_representation(),
            )
        });
        validators.push(quote! {
            let field_start = ::core::mem::offset_of!(Self, #member);
            let Some(field_end) = field_start.checked_add(::core::mem::size_of::<#ty>()) else {
                return false;
            };
            let Some(#local) = bytes.get(field_start..field_end) else {
                return false;
            };
            if !<#ty as #krate::payload::stable::StablePayloadField>::validate_field_bytes(#local) {
                return false;
            }
        });
    }

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let registration = if inventory {
        if !input.generics.params.is_empty() {
            return Err(syn::Error::new_spanned(&input.generics,
                "inventory requires a concrete type; use register_native_type! for a generic instantiation"));
        }
        quote! { #krate::register_native_type!(#name); }
    } else {
        quote!()
    };
    let always_valid_expr = if always_valid.is_empty() {
        quote!(true)
    } else {
        quote!(true #(&& #always_valid)*)
    };

    let _ = fields;
    let layout_assert = if input.generics.params.is_empty() {
        quote! {
            const _: () = {
                let _ = <#name as #krate::payload::stable::StablePayloadField>::FIELD_BITS_ALWAYS_VALID;
            };
        }
    } else {
        quote!()
    };
    Ok(quote! {
        #layout_assert
        #registration
        // SAFETY: repr(C)/transparent fixes layout, the assertions reject drop
        // glue and implicit padding, and each field's checked implementation
        // provides the complete representation and bit-validity contract.
        unsafe impl #impl_generics #krate::payload::stable::StablePayloadField
            for #name #ty_generics #where_clause
        {
            const FIELD_BITS_ALWAYS_VALID: bool = {
                assert!(!::core::mem::needs_drop::<Self>(), "StablePayload cannot have drop glue");
                assert!(::core::mem::size_of::<Self>() == (0_usize #(+ #field_sizes)*),
                    "StablePayload requires explicit padding fields");
                #always_valid_expr
            };

            fn field_representation() -> #krate::NativeRepresentation {
                let _ = Self::FIELD_BITS_ALWAYS_VALID;
                #krate::NativeRepresentation::new(
                    #type_name, #revision,
                    ::core::mem::size_of::<Self>() as u64,
                    ::core::mem::align_of::<Self>() as u64,
                    #krate::NativeByteOrder::native(),
                    vec![#(#representations),*],
                ).expect("compiler-proven stable representation")
            }

            fn validate_field_bytes(bytes: &[u8]) -> bool {
                let _ = Self::FIELD_BITS_ALWAYS_VALID;
                if bytes.len() != ::core::mem::size_of::<Self>() {
                    return false;
                }
                #(#validators)*
                true
            }
        }

        // SAFETY: The field implementation above validates every represented
        // byte and establishes a fixed, fully initialized, pointer-free layout.
        unsafe impl #impl_generics #krate::payload::stable::StablePayload
            for #name #ty_generics #where_clause
        {
            const TYPE_NAME: &'static str = #type_name;
            const REVISION: u32 = #revision;
        }
    })
}

/// SDK path used by generated code: `::up_rust`, unless a consumer that renamed
/// the dependency selects it with `#[stable_payload(crate = "path")]`.
fn sdk_path(attrs: &[syn::Attribute]) -> syn::Result<syn::Path> {
    let mut path = None;
    for attr in attrs {
        if !attr.path().is_ident("stable_payload") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("crate") {
                let value: LitStr = meta.value()?.parse()?;
                path = Some(value.parse::<syn::Path>()?);
            } else if meta.input.peek(syn::Token![=]) {
                // Other options are validated by `StablePayload`; skip their values.
                let _: syn::Lit = meta.value()?.parse()?;
            }
            Ok(())
        })?;
    }
    Ok(path.unwrap_or_else(|| syn::parse_quote!(::up_rust)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializer_rejects_generic_structs() {
        let input = syn::parse_quote!(
            struct Generic<T> {
                value: T,
            }
        );
        assert_eq!(
            derive_stable_payload_init_impl(input)
                .unwrap_err()
                .to_string(),
            "StablePayloadInit does not support generic payload structs"
        );
    }

    #[test]
    fn initializer_rejects_non_structs() {
        let input = syn::parse_quote!(
            enum Choice {
                Value,
            }
        );
        assert_eq!(
            derive_stable_payload_init_impl(input)
                .unwrap_err()
                .to_string(),
            "StablePayloadInit can only be derived for structs"
        );
    }

    #[test]
    fn initializer_rejects_empty_structs() {
        let input = syn::parse_quote!(
            struct Empty;
        );
        assert_eq!(
            derive_stable_payload_init_impl(input)
                .unwrap_err()
                .to_string(),
            "StablePayloadInit requires at least one field"
        );
    }

    #[test]
    fn stable_payload_requires_a_deployment_name() {
        let input = syn::parse_quote!(
            #[repr(C)]
            struct Unnamed {
                value: u32,
            }
        );
        assert_eq!(
            derive_stable_payload_impl(input).unwrap_err().to_string(),
            "StablePayload requires #[stable_payload(type_name = \"...\")]"
        );
    }

    #[test]
    fn stable_payload_rejects_unknown_options() {
        let input = syn::parse_quote!(
            #[repr(C)]
            #[stable_payload(unknown = 1)]
            struct Invalid {
                value: u32,
            }
        );
        assert_eq!(
            derive_stable_payload_impl(input).unwrap_err().to_string(),
            "unsupported stable_payload option"
        );
    }

    #[test]
    fn stable_payload_rejects_non_structs() {
        let input = syn::parse_quote!(
            #[repr(C)]
            #[stable_payload(type_name = "example.Choice")]
            enum Choice {
                Value,
            }
        );
        assert_eq!(
            derive_stable_payload_impl(input).unwrap_err().to_string(),
            "StablePayload can only be derived for structs"
        );
    }

    #[test]
    fn renamed_sdk_path_reaches_both_derives() {
        let input: DeriveInput = syn::parse_quote!(
            #[repr(C)]
            #[stable_payload(type_name = "example.Renamed", crate = "sdk")]
            struct Renamed {
                value: u32,
            }
        );
        let payload = derive_stable_payload_impl(input.clone())
            .unwrap()
            .to_string();
        let init = derive_stable_payload_init_impl(input).unwrap().to_string();
        for generated in [payload, init] {
            assert!(generated.contains("sdk :: payload :: stable"));
            assert!(!generated.contains(":: up_rust ::"));
        }
    }
}
