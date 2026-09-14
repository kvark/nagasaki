//! Shader modules.
//!
//! These are compiled twice: `rustc` checks them as ordinary Rust, and the
//! build script reads the same files and transpiles them to WGSL.
//!
//! A shader keeps WGSL's naming — lowercase types, lowercase globals, and
//! resources nothing on the CPU ever reads — and WGSL has no opinion on a
//! parameter a function does not read, so the lints that would otherwise fire
//! on every one of them are turned off here for the whole subtree.
#![allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code,
    unused_imports,
    unused_variables
)]

pub mod common;
pub mod sprite;
pub mod tonemap;
