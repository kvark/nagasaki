use naga::{
    Block, Function, FunctionArgument, FunctionResult, Handle, Module, Scalar, Span, Statement,
    Type, TypeInner,
};
use syn::{FnArg, Item, ItemFn, ReturnType, Signature};

use crate::Error;

mod emit;
mod env;
mod expr;
mod stmt;

use emit::item_kind;
use env::{Env, Slot};
use stmt::lower_block;

pub struct Context {
    pub module: Module,
}

impl Context {
    pub fn new() -> Self {
        Self {
            module: Module::default(),
        }
    }

    pub fn lower_file(&mut self, file: syn::File) -> Result<(), Error> {
        for item in file.items {
            match item {
                Item::Fn(func) => {
                    self.lower_fn(func)?;
                }
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

    pub(super) fn lower_type(&mut self, ty: &syn::Type) -> Result<Handle<Type>, Error> {
        let ident = match ty {
            syn::Type::Path(path) if path.qself.is_none() => path
                .path
                .get_ident()
                .ok_or_else(|| Error::UnsupportedType("path type".into()))?,
            _ => return Err(Error::UnsupportedType("non-path type".into())),
        };
        match ident.to_string().as_str() {
            "f32" => Ok(self.intern_scalar(Scalar::F32)),
            "u32" => Ok(self.intern_scalar(Scalar::U32)),
            "i32" => Ok(self.intern_scalar(Scalar::I32)),
            "bool" => Ok(self.intern_scalar(Scalar::BOOL)),
            other => Err(Error::UnsupportedType(other.into())),
        }
    }

    fn lower_fn(&mut self, item: ItemFn) -> Result<Handle<Function>, Error> {
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
        lower_signature(self, &mut function, &item.sig, &mut env)?;
        let mut body = Block::new();
        env.push_scope();
        let tail = lower_block(self, &mut function, &mut body, &item.block, &mut env)?;
        env.pop_scope();
        if let Some((value, _)) = tail {
            body.push(Statement::Return { value: Some(value) }, Span::UNDEFINED);
        }
        function.body = body;
        Ok(self.module.functions.append(function, Span::UNDEFINED))
    }
}

fn lower_signature(
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
                let expr = function.expressions.append(
                    naga::Expression::FunctionArgument(index),
                    Span::UNDEFINED,
                );
                env.push(name, Slot::Value(expr), ty);
            }
        }
    }
    Ok(())
}
