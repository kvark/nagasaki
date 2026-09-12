//! Shared helpers for the integration tests.
//!
//! Every test wants one of three things: the WGSL a source lowers to, the fact
//! that it lowers to *something* Naga accepts, or the message it is rejected
//! with.

#![allow(dead_code)]

use nagasaki::{parse_str, to_wgsl, validate, validate_unbound};

/// Lower, validate, and emit WGSL. Panics with the reason on any failure.
pub fn roundtrip(src: &str) -> String {
    let module = parse_str(src).unwrap_or_else(|e| panic!("parse: {e}\n{src}"));
    let info = validate(&module).unwrap_or_else(|e| panic!("validate: {e}\n{src}"));
    to_wgsl(&module, &info).unwrap_or_else(|e| panic!("wgsl: {e}\n{src}"))
}

/// Like `roundtrip`, for a module whose `@group`/`@binding` the host assigns.
pub fn roundtrip_unbound(src: &str) -> String {
    let module = parse_str(src).unwrap_or_else(|e| panic!("parse: {e}\n{src}"));
    let info = validate_unbound(&module).unwrap_or_else(|e| panic!("validate: {e}\n{src}"));
    to_wgsl(&module, &info).unwrap_or_else(|e| panic!("wgsl: {e}\n{src}"))
}

/// Lower and validate, ignoring the generated WGSL.
pub fn validate_only(src: &str) {
    let module = parse_str(src).unwrap_or_else(|e| panic!("parse: {e}\n{src}"));
    validate(&module).unwrap_or_else(|e| panic!("validate: {e}\n{src}"));
}

/// The message `src` is rejected with. Panics if it is accepted instead.
pub fn reject(src: &str) -> String {
    match parse_str(src) {
        Ok(_) => panic!("expected an error, but this was accepted:\n{src}"),
        Err(e) => e.to_string(),
    }
}

/// A module's interface, rendered so two modules built from different sources
/// can be compared: globals, entry points, and the types they carry.
///
/// Comparing the emitted WGSL directly would compare Naga's temporary names,
/// which follow expression ordering and say nothing about equivalence.
pub fn interface(module: &naga::Module) -> String {
    let mut out = String::new();
    let mut globals: Vec<String> = module
        .global_variables
        .iter()
        .map(|(_, var)| {
            format!(
                "global {} : {} [{:?}]",
                var.name.as_deref().unwrap_or("?"),
                type_name(module, var.ty),
                var.space
            )
        })
        .collect();
    globals.sort();
    for line in globals {
        out.push_str(&line);
        out.push('\n');
    }
    for (_, function) in module.functions.iter() {
        out.push_str(&format!("fn {}(", function.name.as_deref().unwrap_or("?")));
        for arg in &function.arguments {
            out.push_str(&format!("{}, ", type_name(module, arg.ty)));
        }
        match &function.result {
            Some(result) => out.push_str(&format!(") -> {}\n", type_name(module, result.ty))),
            None => out.push_str(")\n"),
        }
    }
    for point in &module.entry_points {
        out.push_str(&format!("entry {} {:?}(", point.name, point.stage));
        // Argument names are not part of the interface -- Blade matches vertex
        // attributes by struct *field* name, which type_name still carries --
        // and a port sometimes has to rename one around a Rust keyword.
        for arg in &point.function.arguments {
            out.push_str(&format!(
                "{} {:?}, ",
                type_name(module, arg.ty),
                arg.binding
            ));
        }
        match &point.function.result {
            Some(result) => out.push_str(&format!(
                ") -> {} {:?}\n",
                type_name(module, result.ty),
                result.binding
            )),
            None => out.push_str(")\n"),
        }
    }
    out
}

/// A structural name for a type: two modules never share type handles, so the
/// shape has to be spelled out.
fn type_name(module: &naga::Module, ty: naga::Handle<naga::Type>) -> String {
    use naga::TypeInner as Ti;
    match module.types[ty].inner {
        Ti::Scalar(s) => format!("{:?}{}", s.kind, s.width * 8),
        Ti::Vector { size, scalar } => {
            format!("vec{}<{:?}{}>", size as u32, scalar.kind, scalar.width * 8)
        }
        Ti::Matrix {
            columns,
            rows,
            scalar,
        } => format!(
            "mat{}x{}<{:?}{}>",
            columns as u32,
            rows as u32,
            scalar.kind,
            scalar.width * 8
        ),
        Ti::Array { base, size, .. } => match size {
            naga::ArraySize::Constant(n) => {
                format!("[{}; {}]", type_name(module, base), n.get())
            }
            _ => format!("[{}]", type_name(module, base)),
        },
        Ti::Struct { ref members, .. } => {
            let fields: Vec<String> = members
                .iter()
                .map(|m| {
                    format!(
                        "{}: {} {:?}",
                        m.name.as_deref().unwrap_or("?"),
                        type_name(module, m.ty),
                        m.binding
                    )
                })
                .collect();
            format!(
                "struct {} {{ {} }}",
                module.types[ty].name.as_deref().unwrap_or("?"),
                fields.join(", ")
            )
        }
        Ti::Image {
            dim,
            arrayed,
            class,
        } => format!("texture {dim:?} arrayed={arrayed} {class:?}"),
        Ti::Sampler { comparison } => format!("sampler comparison={comparison}"),
        ref other => format!("{other:?}"),
    }
}
