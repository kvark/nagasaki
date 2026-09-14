//! Attribute macros that make the shader dialect legal Rust.
//!
//! `#[vertex]`, `#[location(0)]`, `#[builtin(position)]` are not Rust
//! attributes, and `rustc` rejects an attribute it cannot resolve — including
//! on a function parameter or a struct field, where nothing else can introduce
//! one. An attribute macro on the enclosing item is the only thing that can:
//! it receives the item with its inner attributes attached, and can take them
//! off before handing the item back.
//!
//! So these macros carry no meaning. They strip the shader attributes and emit
//! what is left. The meaning lives in the transpiler, which reads the same file
//! as text and does look at them.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, ItemFn, ItemStruct};

/// Attributes that belong to the shader and not to Rust.
fn is_shader_attr(attr: &syn::Attribute) -> bool {
    const NAMES: &[&str] = &[
        "location",
        "builtin",
        "workgroup_size",
        "output",
        "flat",
        "interpolate",
        "group",
        "binding",
        "uniform",
        "storage",
        "workgroup",
        "private",
        "invariant",
    ];
    NAMES.iter().any(|name| attr.path().is_ident(name))
}

fn strip(attrs: &mut Vec<syn::Attribute>) {
    attrs.retain(|attr| !is_shader_attr(attr));
}

/// Take the shader attributes off a function, its parameters included, and
/// wrap the body so it may touch the `static mut` a writable resource is.
fn clean_fn(mut item: ItemFn) -> TokenStream {
    strip(&mut item.attrs);
    for arg in &mut item.sig.inputs {
        match arg {
            FnArg::Typed(pat) => strip(&mut pat.attrs),
            FnArg::Receiver(receiver) => strip(&mut receiver.attrs),
        }
    }

    // Assigning through a shared `static` is not something Rust allows however
    // the type is arranged, so a writable resource is a `static mut` and the
    // shader source should not have to say `unsafe` about work that is only
    // ever done on a GPU.
    let block = &item.block;
    let wrapped: syn::Block = syn::parse_quote!({
        #[allow(unused_unsafe)]
        unsafe #block
    });
    *item.block = wrapped;

    quote!(
        #[allow(
            non_snake_case,
            non_upper_case_globals,
            unused_variables,
            unused_mut,
            unused_parens,
            dead_code,
            clippy::all
        )]
        #item
    )
    .into()
}

/// A vertex entry point.
#[proc_macro_attribute]
pub fn vertex(_args: TokenStream, input: TokenStream) -> TokenStream {
    clean_fn(parse_macro_input!(input as ItemFn))
}

/// A fragment entry point.
#[proc_macro_attribute]
pub fn fragment(_args: TokenStream, input: TokenStream) -> TokenStream {
    clean_fn(parse_macro_input!(input as ItemFn))
}

/// A compute entry point.
#[proc_macro_attribute]
pub fn compute(_args: TokenStream, input: TokenStream) -> TokenStream {
    clean_fn(parse_macro_input!(input as ItemFn))
}

/// A shader function that is not an entry point.
///
/// Only needed on a helper that writes to a resource, which is what the
/// `unsafe` wrapper is for. A helper that only reads needs nothing.
#[proc_macro_attribute]
pub fn shader(_args: TokenStream, input: TokenStream) -> TokenStream {
    clean_fn(parse_macro_input!(input as ItemFn))
}

/// A struct whose fields carry `#[location]` or `#[builtin]` bindings: a
/// vertex output, a fragment input, or a set of render targets.
#[proc_macro_attribute]
pub fn io(_args: TokenStream, input: TokenStream) -> TokenStream {
    let mut item = parse_macro_input!(input as ItemStruct);
    strip(&mut item.attrs);
    for field in &mut item.fields {
        strip(&mut field.attrs);
    }
    quote!(
        #[allow(non_snake_case, dead_code)]
        #[derive(Clone, Copy, Debug, Default)]
        #item
    )
    .into()
}
