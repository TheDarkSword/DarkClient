//! Derive macro for `MappedObject`.
//!
//! Generates, for a Rust wrapper around a JVM object, the `MappedObject` trait
//! impl plus `PartialEq` / `Eq` (value equality via Java `Object.equals`).

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Error};

/// Derives `MappedObject` for a wrapper struct.
///
/// The struct must have a `jni_ref` field and a `#[mapped(class = <Variant>)]`
/// attribute naming its `MinecraftClassType`:
///
/// ```ignore
/// #[derive(MappedObject)]
/// #[mapped(class = Entity)]
/// pub struct Entity {
///     jni_ref: GlobalRef,
/// }
/// ```
#[proc_macro_derive(MappedObject, attributes(mapped))]
pub fn derive_mapped_object(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // The struct must carry a `jni_ref` field — that is what the impl reads.
    match &input.data {
        Data::Struct(data) => {
            let has_jni_ref = data
                .fields
                .iter()
                .any(|field| field.ident.as_ref().is_some_and(|id| id == "jni_ref"));
            if !has_jni_ref {
                return Error::new_spanned(
                    name,
                    "#[derive(MappedObject)] requires a `jni_ref` field",
                )
                .to_compile_error()
                .into();
            }
        }
        _ => {
            return Error::new_spanned(name, "#[derive(MappedObject)] supports structs only")
                .to_compile_error()
                .into();
        }
    }

    // Extract the class variant from `#[mapped(class = ...)]`.
    let mut class = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("mapped") {
            continue;
        }
        let parsed = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("class") {
                class = Some(meta.value()?.parse::<syn::Ident>()?);
                Ok(())
            } else {
                Err(meta.error("expected `class = <MinecraftClassType variant>`"))
            }
        });
        if let Err(error) = parsed {
            return error.to_compile_error().into();
        }
    }

    let class = match class {
        Some(class) => class,
        None => {
            return Error::new_spanned(
                name,
                "#[derive(MappedObject)] requires #[mapped(class = <variant>)]",
            )
            .to_compile_error()
            .into();
        }
    };

    quote! {
        impl crate::mapping::MappedObject for #name {
            fn jni_ref(&self) -> &::jni::objects::GlobalRef {
                &self.jni_ref
            }

            fn class_type() -> crate::mapping::MinecraftClassType {
                crate::mapping::MinecraftClassType::#class
            }
        }

        impl ::core::cmp::PartialEq for #name {
            fn eq(&self, other: &Self) -> bool {
                crate::mapping::MappedObject::equals(self, other)
            }
        }

        impl ::core::cmp::Eq for #name {}
    }
    .into()
}
