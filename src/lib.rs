//! Native Rust → [`naga::Module`] frontend.
//!
//! v0 dialect: free functions, scalars (`f32` / `u32` / `i32` / `bool`),
//! vectors (`vec2`/`vec3`/`vec4` and `vecN<T>`),
//! matrices (`mat2`/`mat3`/`mat4` and `matCxR`), literals, unary/binary operators,
//! `as` casts, `let`, assignment, `if`/`else`, `loop`/`while`, `return`, and
//! `#[vertex]`/`#[fragment]`/`#[compute]` entry points,
//! `#[group]`/`#[binding]` globals, and named structs — including structs that
//! carry `#[location]` / `#[builtin]` bindings on their fields, as vertex
//! outputs and fragment inputs do.
//! No references, methods, or generics yet.
//!
//! Operand typing mirrors Naga's own rules, so anything [`parse_str`] accepts
//! is a module [`validate`] accepts; untyped integer literals take their type
//! from context, as Rust's inference would.

mod error;
mod lower;

pub use error::Error;
pub use naga;

use lower::Context;

/// Parse a Rust source string into a Naga module.
pub fn parse_str(source: &str) -> Result<naga::Module, Error> {
    let file: syn::File = syn::parse_str(source)?;
    let mut ctx = Context::new();
    ctx.lower_file(file)?;
    Ok(ctx.module)
}

/// Validate `module` with default Naga flags.
pub fn validate(
    module: &naga::Module,
) -> Result<naga::valid::ModuleInfo, Box<dyn std::error::Error + Send + Sync>> {
    validate_with(module, naga::valid::ValidationFlags::all())
}

/// Validate a module whose resource bindings the host assigns.
///
/// Some engines — Blade among them — leave `@group`/`@binding` out of the
/// shader and fill them in at pipeline creation, matching globals up by name.
/// A module for one of those has globals with no binding, which the default
/// flags reject.
pub fn validate_unbound(
    module: &naga::Module,
) -> Result<naga::valid::ModuleInfo, Box<dyn std::error::Error + Send + Sync>> {
    validate_with(
        module,
        naga::valid::ValidationFlags::all() ^ naga::valid::ValidationFlags::BINDINGS,
    )
}

fn validate_with(
    module: &naga::Module,
    flags: naga::valid::ValidationFlags,
) -> Result<naga::valid::ModuleInfo, Box<dyn std::error::Error + Send + Sync>> {
    Ok(naga::valid::Validator::new(flags, naga::valid::Capabilities::empty()).validate(module)?)
}

/// Emit WGSL for a validated module.
pub fn to_wgsl(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
) -> Result<String, naga::back::wgsl::Error> {
    naga::back::wgsl::write_string(module, info, naga::back::wgsl::WriterFlags::empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_f32() {
        let module = parse_str("fn add(a: f32, b: f32) -> f32 { a + b }").unwrap();
        let info = validate(&module).expect("naga validation");
        let wgsl = to_wgsl(&module, &info).expect("wgsl");
        assert!(wgsl.contains("fn add"), "{wgsl}");
        assert!(wgsl.contains('+'), "{wgsl}");
    }
}
