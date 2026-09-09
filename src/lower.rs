use naga::{
    proc::Emitter, BinaryOperator, Expression, Function, FunctionArgument, FunctionResult, Handle,
    Literal, Module, Scalar, Span, Statement, Type, TypeInner, UnaryOperator,
};
use syn::{
    BinOp, Block as SynBlock, Expr, FnArg, Item, ItemFn, Pat, ReturnType, Signature, Stmt,
    Type as SynType,
};

use crate::Error;

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

    fn intern_scalar(&mut self, scalar: Scalar) -> Handle<Type> {
        // UniqueArena interns by value, so repeated f32/u32/i32/bool collapse.
        self.module.types.insert(
            Type {
                name: None,
                inner: TypeInner::Scalar(scalar),
            },
            Span::UNDEFINED,
        )
    }

    fn lower_type(&mut self, ty: &SynType) -> Result<Handle<Type>, Error> {
        let ident = match ty {
            SynType::Path(path) if path.qself.is_none() => path
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
        lower_block(self, &mut function, &item.block, &mut env)?;
        Ok(self.module.functions.append(function, Span::UNDEFINED))
    }
}

#[derive(Default)]
struct Env {
    values: Vec<(String, Handle<Expression>)>,
}

impl Env {
    fn push(&mut self, name: String, expr: Handle<Expression>) {
        self.values.push((name, expr));
    }

    fn lookup(&self, name: &str) -> Option<Handle<Expression>> {
        self.values
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, h)| *h)
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
                    Pat::Ident(ident) if ident.by_ref.is_none() && ident.mutability.is_none() => {
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
                    .append(Expression::FunctionArgument(index), Span::UNDEFINED);
                env.push(name, expr);
            }
        }
    }
    Ok(())
}

fn lower_block(
    ctx: &mut Context,
    function: &mut Function,
    block: &SynBlock,
    env: &mut Env,
) -> Result<(), Error> {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Expr(Expr::Return(ret), _) => {
                let value = match ret.expr.as_deref() {
                    Some(expr) => Some(lower_expr(ctx, function, expr, env)?),
                    None => None,
                };
                function
                    .body
                    .push(Statement::Return { value }, Span::UNDEFINED);
            }
            Stmt::Expr(expr, None) => {
                let value = lower_expr(ctx, function, expr, env)?;
                function
                    .body
                    .push(Statement::Return { value: Some(value) }, Span::UNDEFINED);
            }
            Stmt::Expr(expr, Some(_)) => {
                let _ = lower_expr(ctx, function, expr, env)?;
            }
            Stmt::Local(_) => return Err(Error::UnsupportedStmt("let".into())),
            Stmt::Item(_) => return Err(Error::UnsupportedStmt("item in block".into())),
            Stmt::Macro(_) => return Err(Error::UnsupportedStmt("macro".into())),
        }
    }
    Ok(())
}

fn lower_expr(
    ctx: &mut Context,
    function: &mut Function,
    expr: &Expr,
    env: &Env,
) -> Result<Handle<Expression>, Error> {
    match expr {
        Expr::Paren(inner) => lower_expr(ctx, function, &inner.expr, env),
        Expr::Group(inner) => lower_expr(ctx, function, &inner.expr, env),
        Expr::Path(path) => {
            let ident = path
                .path
                .get_ident()
                .ok_or_else(|| Error::UnsupportedExpr("path".into()))?;
            env.lookup(&ident.to_string())
                .ok_or_else(|| Error::UnknownIdent(ident.to_string()))
        }
        Expr::Lit(lit) => lower_lit(function, lit),
        Expr::Binary(bin) => {
            let left = lower_expr(ctx, function, &bin.left, env)?;
            let right = lower_expr(ctx, function, &bin.right, env)?;
            emit(
                function,
                Expression::Binary {
                    op: map_bin_op(&bin.op)?,
                    left,
                    right,
                },
            )
        }
        Expr::Unary(unary) => {
            let inner = lower_expr(ctx, function, &unary.expr, env)?;
            let op = match unary.op {
                syn::UnOp::Neg(_) => UnaryOperator::Negate,
                syn::UnOp::Not(_) => UnaryOperator::LogicalNot,
                _ => return Err(Error::UnsupportedExpr("unary".into())),
            };
            emit(function, Expression::Unary { op, expr: inner })
        }
        _ => Err(Error::UnsupportedExpr(expr_kind(expr))),
    }
}

fn lower_lit(function: &mut Function, lit: &syn::ExprLit) -> Result<Handle<Expression>, Error> {
    let literal = match &lit.lit {
        syn::Lit::Float(f) => {
            if f.suffix() == "f64" {
                return Err(Error::UnsupportedType("f64".into()));
            }
            Literal::F32(f.base10_parse().map_err(Error::from)?)
        }
        syn::Lit::Int(i) => match i.suffix() {
            "" | "i32" => Literal::I32(i.base10_parse().map_err(Error::from)?),
            "u32" => Literal::U32(i.base10_parse().map_err(Error::from)?),
            other => return Err(Error::UnsupportedType(other.into())),
        },
        syn::Lit::Bool(b) => Literal::Bool(b.value()),
        _ => return Err(Error::UnsupportedExpr("literal".into())),
    };
    Ok(function
        .expressions
        .append(Expression::Literal(literal), Span::UNDEFINED))
}

fn map_bin_op(op: &BinOp) -> Result<BinaryOperator, Error> {
    Ok(match op {
        BinOp::Add(_) => BinaryOperator::Add,
        BinOp::Sub(_) => BinaryOperator::Subtract,
        BinOp::Mul(_) => BinaryOperator::Multiply,
        BinOp::Div(_) => BinaryOperator::Divide,
        BinOp::Rem(_) => BinaryOperator::Modulo,
        BinOp::Eq(_) => BinaryOperator::Equal,
        BinOp::Ne(_) => BinaryOperator::NotEqual,
        BinOp::Lt(_) => BinaryOperator::Less,
        BinOp::Le(_) => BinaryOperator::LessEqual,
        BinOp::Gt(_) => BinaryOperator::Greater,
        BinOp::Ge(_) => BinaryOperator::GreaterEqual,
        BinOp::And(_) => BinaryOperator::LogicalAnd,
        BinOp::Or(_) => BinaryOperator::LogicalOr,
        BinOp::BitAnd(_) => BinaryOperator::And,
        BinOp::BitOr(_) => BinaryOperator::InclusiveOr,
        BinOp::BitXor(_) => BinaryOperator::ExclusiveOr,
        BinOp::Shl(_) => BinaryOperator::ShiftLeft,
        BinOp::Shr(_) => BinaryOperator::ShiftRight,
        _ => return Err(Error::UnsupportedBinOp("compound assignment".into())),
    })
}

fn emit(function: &mut Function, expr: Expression) -> Result<Handle<Expression>, Error> {
    let mut emitter = Emitter::default();
    emitter.start(&function.expressions);
    let handle = function.expressions.append(expr, Span::UNDEFINED);
    function.body.extend(emitter.finish(&function.expressions));
    Ok(handle)
}

fn item_kind(item: &Item) -> String {
    match item {
        Item::Const(_) => "const",
        Item::Enum(_) => "enum",
        Item::ExternCrate(_) => "extern crate",
        Item::Fn(_) => "fn",
        Item::ForeignMod(_) => "extern",
        Item::Impl(_) => "impl",
        Item::Macro(_) => "macro",
        Item::Mod(_) => "mod",
        Item::Static(_) => "static",
        Item::Struct(_) => "struct",
        Item::Trait(_) => "trait",
        Item::TraitAlias(_) => "trait alias",
        Item::Type(_) => "type alias",
        Item::Union(_) => "union",
        Item::Use(_) => "use",
        Item::Verbatim(_) => "verbatim",
        _ => "item",
    }
    .into()
}

fn expr_kind(expr: &Expr) -> String {
    match expr {
        Expr::Assign(_) => "assignment",
        Expr::Call(_) => "call",
        Expr::Field(_) => "field",
        Expr::If(_) => "if",
        Expr::Index(_) => "index",
        Expr::Loop(_) => "loop",
        Expr::MethodCall(_) => "method",
        Expr::Reference(_) => "reference",
        Expr::Struct(_) => "struct literal",
        Expr::While(_) => "while",
        _ => "expression",
    }
    .into()
}
