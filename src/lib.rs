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

/// A Naga validation failure, with the reason it gives.
///
/// Naga puts the useful part of a validation error in the source chain: the top
/// level says only which global or function is invalid. Printing this prints
/// the whole chain, so the actual complaint is visible.
#[derive(Debug)]
pub struct ValidationError(Box<naga::WithSpan<naga::valid::ValidationError>>);

impl ValidationError {
    /// The underlying Naga error, spans included.
    pub fn into_inner(self) -> naga::WithSpan<naga::valid::ValidationError> {
        *self.0
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)?;
        let mut source = std::error::Error::source(&*self.0);
        while let Some(err) = source {
            write!(f, ": {err}")?;
            source = err.source();
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.0)
    }
}

/// Validate `module` with default Naga flags and no extra capabilities.
pub fn validate(module: &naga::Module) -> Result<naga::valid::ModuleInfo, ValidationError> {
    validate_with(
        module,
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
}

/// Validate a module whose resource bindings the host assigns.
///
/// Some engines — Blade among them — leave `@group`/`@binding` out of the
/// shader and fill them in at pipeline creation, matching globals up by name.
/// A module for one of those has globals with no binding, which the default
/// flags reject.
pub fn validate_unbound(module: &naga::Module) -> Result<naga::valid::ModuleInfo, ValidationError> {
    validate_with(
        module,
        naga::valid::ValidationFlags::all() ^ naga::valid::ValidationFlags::BINDINGS,
        naga::valid::Capabilities::empty(),
    )
}

/// Validate with the flags and capabilities of your choosing.
///
/// Some of the dialect needs a capability the defaults leave off — ray queries
/// need [`Capabilities::RAY_QUERY`] — and a host validating against a real
/// device has its own set anyway.
///
/// [`Capabilities::RAY_QUERY`]: naga::valid::Capabilities::RAY_QUERY
pub fn validate_with(
    module: &naga::Module,
    flags: naga::valid::ValidationFlags,
    capabilities: naga::valid::Capabilities,
) -> Result<naga::valid::ModuleInfo, ValidationError> {
    naga::valid::Validator::new(flags, capabilities)
        .validate(module)
        .map_err(|e| ValidationError(Box::new(e)))
}

/// Emit WGSL for a validated module.
///
/// Naga's WGSL backend has no spelling for a ray query and panics on one, so
/// that case is turned away first. The module is still good: Naga's SPIR-V,
/// MSL and HLSL backends handle ray queries, and a host that takes a
/// `naga::Module` directly never needs this function.
pub fn to_wgsl(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
) -> Result<String, naga::back::wgsl::Error> {
    if let Some(name) = uses_ray_query(module) {
        return Err(naga::back::wgsl::Error::Unimplemented(format!(
            "`{name}` traces a ray query, which Naga's WGSL backend cannot write"
        )));
    }
    naga::back::wgsl::write_string(module, info, naga::back::wgsl::WriterFlags::empty())
}

/// The name of the first function that traces a ray query, if any does.
fn uses_ray_query(module: &naga::Module) -> Option<String> {
    fn in_block(block: &naga::Block) -> bool {
        block.iter().any(|stmt| match stmt {
            naga::Statement::RayQuery { .. } => true,
            naga::Statement::Block(inner) => in_block(inner),
            naga::Statement::If { accept, reject, .. } => in_block(accept) || in_block(reject),
            naga::Statement::Loop {
                body, continuing, ..
            } => in_block(body) || in_block(continuing),
            naga::Statement::Switch { cases, .. } => cases.iter().any(|c| in_block(&c.body)),
            _ => false,
        })
    }
    let functions = module
        .functions
        .iter()
        .map(|(_, f)| (f.name.clone().unwrap_or_default(), &f.body));
    let entries = module
        .entry_points
        .iter()
        .map(|e| (e.name.clone(), &e.function.body));
    functions
        .chain(entries)
        .find(|(_, body)| in_block(body))
        .map(|(name, _)| name)
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
