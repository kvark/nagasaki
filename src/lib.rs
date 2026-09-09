//! Native Rust → [`naga::Module`] frontend.
//!
//! v0 dialect: free functions, scalar types (`f32` / `u32` / `i32` / `bool`),
//! literals, unary/binary operators, and `return`. No references, methods,
//! generics, or control flow yet.

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
    Ok(naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(module)?)
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
