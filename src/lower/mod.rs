use naga::{
    Block, Function, FunctionArgument, FunctionResult, Handle, Module, Scalar, Span, Statement,
    Type, TypeInner, VectorSize,
};
use syn::{FnArg, Item, ItemFn, ReturnType, Signature};

use crate::Error;

mod call;
mod emit;
mod entry;
mod env;
mod expr;
mod global;
mod matrix;
mod stmt;
mod vector;

use emit::item_kind;
use env::{Env, Slot};
use stmt::lower_block;

pub struct Context {
    pub module: Module,
    pub(super) globals: Vec<global::GlobalInfo>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            module: Module::default(),
            globals: Vec::new(),
        }
    }

    pub fn lower_file(&mut self, file: syn::File) -> Result<(), Error> {
        for item in file.items {
            match item {
                Item::Fn(func) => {
                    self.lower_fn(func)?;
                }
                Item::Static(st) => global::lower_static(self, st)?,
                Item::ForeignMod(fm) => global::lower_foreign_mod(self, fm)?,
                other => return Err(Error::UnsupportedItem(item_kind(&other))),
            }
        }
        Ok(())
    }

    pub(super) fn intern_scalar(&mut self, scalar: Scalar) -> Handle<Type> {
        self.module.types.insert(
            Type {
                name: None,
                inner: TypeInner::Scalar(scalar),
            },
            Span::UNDEFINED,
        )
    }

    pub(super) fn intern_vector(&mut self, size: VectorSize, scalar: Scalar) -> Handle<Type> {
        self.module.types.insert(
            Type {
                name: None,
                inner: TypeInner::Vector { size, scalar },
            },
            Span::UNDEFINED,
        )
    }

    pub(super) fn as_scalar(&self, ty: Handle<Type>) -> Option<Scalar> {
        match self.module.types[ty].inner {
            TypeInner::Scalar(scalar) => Some(scalar),
            _ => None,
        }
    }

    pub(super) fn as_vector(&self, ty: Handle<Type>) -> Option<(VectorSize, Scalar)> {
        match self.module.types[ty].inner {
            TypeInner::Vector { size, scalar } => Some((size, scalar)),
            _ => None,
        }
    }

    pub(super) fn intern_matrix(
        &mut self,
        columns: VectorSize,
        rows: VectorSize,
        scalar: Scalar,
    ) -> Handle<Type> {
        self.module.types.insert(
            Type {
                name: None,
                inner: TypeInner::Matrix {
                    columns,
                    rows,
                    scalar,
                },
            },
            Span::UNDEFINED,
        )
    }

    pub(super) fn as_matrix(&self, ty: Handle<Type>) -> Option<(VectorSize, VectorSize, Scalar)> {
        match self.module.types[ty].inner {
            TypeInner::Matrix {
                columns,
                rows,
                scalar,
            } => Some((columns, rows, scalar)),
            _ => None,
        }
    }

    pub(super) fn lower_type(&mut self, ty: &syn::Type) -> Result<Handle<Type>, Error> {
        let path = match ty {
            syn::Type::Path(path) if path.qself.is_none() => path,
            _ => return Err(Error::UnsupportedType("non-path type".into())),
        };
        if path.path.segments.len() != 1 {
            return Err(Error::UnsupportedType("path type".into()));
        }
        let seg = &path.path.segments[0];
        let name = seg.ident.to_string();
        let type_arg = match &seg.arguments {
            syn::PathArguments::None => None,
            syn::PathArguments::AngleBracketed(args) if args.args.len() == 1 => {
                match args.args.first() {
                    Some(syn::GenericArgument::Type(inner)) => Some(inner),
                    _ => return Err(Error::UnsupportedType(name)),
                }
            }
            _ => return Err(Error::UnsupportedType(name)),
        };

        if let Some((size, shorthand)) = parse_vec_ident(&name) {
            let scalar = match (shorthand, type_arg) {
                (Some(scalar), None) => scalar,
                (None, None) => Scalar::F32,
                (None, Some(inner)) => lower_scalar_ident(inner)?,
                (Some(_), Some(_)) => return Err(Error::UnsupportedType(name)),
            };
            return Ok(self.intern_vector(size, scalar));
        }

        if let Some((columns, rows, shorthand)) = parse_mat_ident(&name) {
            let scalar = match (shorthand, type_arg) {
                (Some(scalar), None) => scalar,
                (None, None) => Scalar::F32,
                (None, Some(inner)) => lower_scalar_ident(inner)?,
                (Some(_), Some(_)) => return Err(Error::UnsupportedType(name)),
            };
            return Ok(self.intern_matrix(columns, rows, scalar));
        }

        if type_arg.is_some() {
            return Err(Error::UnsupportedType(name));
        }
        match name.as_str() {
            "f32" => Ok(self.intern_scalar(Scalar::F32)),
            "u32" => Ok(self.intern_scalar(Scalar::U32)),
            "i32" => Ok(self.intern_scalar(Scalar::I32)),
            "bool" => Ok(self.intern_scalar(Scalar::BOOL)),
            other => Err(Error::UnsupportedType(other.into())),
        }
    }

    fn lower_fn(&mut self, item: ItemFn) -> Result<(), Error> {
        if !item.sig.generics.params.is_empty() {
            return Err(Error::UnsupportedItem(format!(
                "generic function `{}`",
                item.sig.ident
            )));
        }
        if item.sig.asyncness.is_some() || item.sig.abi.is_some() {
            return Err(Error::UnsupportedItem(format!(
                "async/extern function `{}`",
                item.sig.ident
            )));
        }

        let info = entry::parse_fn_attrs(&item.attrs)?;
        if info.stage.is_some() {
            return entry::lower_entry(self, item, info);
        }
        if info.workgroup_size.is_some() || info.return_binding.is_some() {
            return Err(Error::UnsupportedItem(
                "entry-point attribute on a regular function".into(),
            ));
        }

        let name = item.sig.ident.to_string();
        let result_ty = match &item.sig.output {
            ReturnType::Type(_, ty) => self.lower_type(ty)?,
            ReturnType::Default => return Err(Error::MissingReturnType(name)),
        };

        let mut function = Function {
            name: Some(name),
            arguments: Vec::new(),
            result: Some(FunctionResult {
                ty: result_ty,
                binding: None,
            }),
            ..Default::default()
        };

        let mut env = Env::default();
        global::bind_globals(self, &mut function, &mut env);
        lower_signature(self, &mut function, &item.sig, &mut env)?;
        let mut body = Block::new();
        env.push_scope();
        let tail = lower_block(self, &mut function, &mut body, &item.block, &mut env)?;
        env.pop_scope();
        if let Some((value, _)) = tail {
            body.push(Statement::Return { value: Some(value) }, Span::UNDEFINED);
        }
        function.body = body;
        self.module.functions.append(function, Span::UNDEFINED);
        Ok(())
    }
}

/// Parse `vec2` / `Vec3` / `vec4f` / `vec3i` / `vec2u`.
/// `None` scalar means "default f32, or infer from constructor args".
pub(super) fn parse_vec_ident(name: &str) -> Option<(VectorSize, Option<Scalar>)> {
    match name {
        "vec2" | "Vec2" => Some((VectorSize::Bi, None)),
        "vec3" | "Vec3" => Some((VectorSize::Tri, None)),
        "vec4" | "Vec4" => Some((VectorSize::Quad, None)),
        "vec2f" | "Vec2f" => Some((VectorSize::Bi, Some(Scalar::F32))),
        "vec3f" | "Vec3f" => Some((VectorSize::Tri, Some(Scalar::F32))),
        "vec4f" | "Vec4f" => Some((VectorSize::Quad, Some(Scalar::F32))),
        "vec2i" | "Vec2i" => Some((VectorSize::Bi, Some(Scalar::I32))),
        "vec3i" | "Vec3i" => Some((VectorSize::Tri, Some(Scalar::I32))),
        "vec4i" | "Vec4i" => Some((VectorSize::Quad, Some(Scalar::I32))),
        "vec2u" | "Vec2u" => Some((VectorSize::Bi, Some(Scalar::U32))),
        "vec3u" | "Vec3u" => Some((VectorSize::Tri, Some(Scalar::U32))),
        "vec4u" | "Vec4u" => Some((VectorSize::Quad, Some(Scalar::U32))),
        _ => None,
    }
}

/// Parse `mat2` / `Mat4` / `mat2x3` / `mat4f`.
/// Scalar `None` means default `f32` (or infer from constructor args).
pub(super) fn parse_mat_ident(name: &str) -> Option<(VectorSize, VectorSize, Option<Scalar>)> {
    fn pair(c: char, r: char) -> Option<(VectorSize, VectorSize)> {
        let dim = |ch| match ch {
            '2' => Some(VectorSize::Bi),
            '3' => Some(VectorSize::Tri),
            '4' => Some(VectorSize::Quad),
            _ => None,
        };
        Some((dim(c)?, dim(r)?))
    }

    let (stem, scalar) = match name.as_bytes().last().copied() {
        Some(b'f' | b'F') if name.len() > 1 => (&name[..name.len() - 1], Some(Scalar::F32)),
        _ => (name, None),
    };

    match stem {
        "mat2" | "Mat2" => Some((VectorSize::Bi, VectorSize::Bi, scalar)),
        "mat3" | "Mat3" => Some((VectorSize::Tri, VectorSize::Tri, scalar)),
        "mat4" | "Mat4" => Some((VectorSize::Quad, VectorSize::Quad, scalar)),
        other => {
            // matCxR / MatCxR
            let bytes = other.as_bytes();
            let rest = if let Some(r) = other.strip_prefix("mat") {
                r
            } else if let Some(r) = other.strip_prefix("Mat") {
                r
            } else {
                return None;
            };
            let b = rest.as_bytes();
            if b.len() == 3 && b[1] == b'x' {
                let (columns, rows) = pair(b[0] as char, b[2] as char)?;
                // reject leftover from a bad suffix like mat2x3i until we add integer mats
                let _ = bytes;
                Some((columns, rows, scalar))
            } else {
                None
            }
        }
    }
}

fn lower_scalar_ident(ty: &syn::Type) -> Result<Scalar, Error> {
    let ident = match ty {
        syn::Type::Path(path) if path.qself.is_none() => path
            .path
            .get_ident()
            .ok_or_else(|| Error::UnsupportedType("path type".into()))?,
        _ => return Err(Error::UnsupportedType("non-scalar type argument".into())),
    };
    match ident.to_string().as_str() {
        "f32" => Ok(Scalar::F32),
        "u32" => Ok(Scalar::U32),
        "i32" => Ok(Scalar::I32),
        "bool" => Ok(Scalar::BOOL),
        other => Err(Error::UnsupportedType(other.into())),
    }
}

pub(super) fn lower_signature(
    ctx: &mut Context,
    function: &mut Function,
    sig: &Signature,
    env: &mut Env,
) -> Result<(), Error> {
    for arg in &sig.inputs {
        match arg {
            FnArg::Receiver(_) => return Err(Error::Receiver),
            FnArg::Typed(pat_ty) => {
                let name = match &*pat_ty.pat {
                    syn::Pat::Ident(ident)
                        if ident.by_ref.is_none() && ident.mutability.is_none() =>
                    {
                        ident.ident.to_string()
                    }
                    _ => return Err(Error::PatternParam),
                };
                let ty = ctx.lower_type(&pat_ty.ty)?;
                let index = function.arguments.len() as u32;
                function.arguments.push(FunctionArgument {
                    name: Some(name.clone()),
                    ty,
                    binding: None,
                });
                let expr = function
                    .expressions
                    .append(naga::Expression::FunctionArgument(index), Span::UNDEFINED);
                env.push(name, Slot::Value(expr), ty);
            }
        }
    }
    Ok(())
}
