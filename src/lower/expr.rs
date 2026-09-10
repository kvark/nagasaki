use naga::{
    BinaryOperator, Block, Expression, Function, Handle, Literal, Scalar, Span, Statement, Type,
    TypeInner, UnaryOperator,
};
use syn::{BinOp, Expr};

use super::call::lower_call;
use super::emit::{emit, expr_kind};
use super::env::{Env, Slot};
use super::stmt::{lower_block, lower_if_expr};
use super::vector::{lower_field, lower_index, splat_mix};
use super::Context;
use crate::Error;

pub(super) fn lower_expr(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    expr: &Expr,
    env: &mut Env,
) -> Result<(Handle<Expression>, Handle<Type>), Error> {
    match expr {
        Expr::Paren(inner) => lower_expr(ctx, function, body, &inner.expr, env),
        Expr::Group(inner) => lower_expr(ctx, function, body, &inner.expr, env),
        Expr::Path(path) => {
            let ident = path
                .path
                .get_ident()
                .ok_or_else(|| Error::UnsupportedExpr("path".into()))?;
            let binding = env
                .lookup(&ident.to_string())
                .ok_or_else(|| Error::UnknownIdent(ident.to_string()))?;
            let ty = binding.ty;
            let expr = match binding.slot {
                Slot::Value(handle) => handle,
                Slot::Ptr(pointer) => emit(function, body, Expression::Load { pointer })?,
            };
            Ok((expr, ty))
        }
        Expr::Lit(lit) => lower_lit(ctx, function, lit),
        Expr::Binary(bin) => {
            if let Some(op) = map_compound_op(&bin.op) {
                return lower_assign(ctx, function, body, &bin.left, &bin.right, Some(op), env);
            }
            let (mut left, mut left_ty) = lower_expr(ctx, function, body, &bin.left, env)?;
            let (mut right, mut right_ty) = lower_expr(ctx, function, body, &bin.right, env)?;
            let op = map_bin_op(&bin.op)?;
            splat_mix(ctx, function, body, &mut left, &mut left_ty, &mut right, &mut right_ty)?;
            let ty = bin_result_ty(ctx, op, left_ty, right_ty)?;
            let handle = emit(function, body, Expression::Binary { op, left, right })?;
            Ok((handle, ty))
        }
        Expr::Unary(unary) => {
            let (inner, inner_ty) = lower_expr(ctx, function, body, &unary.expr, env)?;
            let (op, ty) = match unary.op {
                syn::UnOp::Neg(_) => (UnaryOperator::Negate, inner_ty),
                syn::UnOp::Not(_) => {
                    let is_bool = matches!(
                        ctx.as_scalar(inner_ty),
                        Some(s) if s == Scalar::BOOL
                    ) || matches!(
                        ctx.as_vector(inner_ty),
                        Some((_, s)) if s == Scalar::BOOL
                    );
                    if !is_bool {
                        return Err(Error::TypeMismatch);
                    }
                    (UnaryOperator::LogicalNot, inner_ty)
                }
                _ => return Err(Error::UnsupportedExpr("unary".into())),
            };
            let handle = emit(function, body, Expression::Unary { op, expr: inner })?;
            Ok((handle, ty))
        }
        Expr::Assign(assign) => {
            lower_assign(ctx, function, body, &assign.left, &assign.right, None, env)
        }
        Expr::If(if_expr) => lower_if_expr(ctx, function, body, if_expr, env),
        Expr::Block(b) => {
            env.push_scope();
            let tail = lower_block(ctx, function, body, &b.block, env)?;
            env.pop_scope();
            tail.ok_or(Error::MissingBlockValue)
        }
        Expr::Call(call) => lower_call(ctx, function, body, call, env),
        Expr::Field(field) => lower_field(ctx, function, body, field, env),
        Expr::Index(index) => lower_index(ctx, function, body, index, env),
        Expr::Struct(lit) => super::structure::lower_struct_lit(ctx, function, body, lit, env),
        _ => Err(Error::UnsupportedExpr(expr_kind(expr))),
    }
}

fn lower_assign(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    left: &Expr,
    right: &Expr,
    compound: Option<BinaryOperator>,
    env: &mut Env,
) -> Result<(Handle<Expression>, Handle<Type>), Error> {
    let name = match left {
        Expr::Path(path) => path
            .path
            .get_ident()
            .ok_or(Error::InvalidAssignTarget)?
            .to_string(),
        Expr::Paren(inner) => {
            return lower_assign(ctx, function, body, &inner.expr, right, compound, env);
        }
        _ => return Err(Error::InvalidAssignTarget),
    };
    let binding = env
        .lookup(&name)
        .ok_or_else(|| Error::UnknownIdent(name.clone()))?;
    let ty = binding.ty;
    let pointer = match binding.slot {
        Slot::Ptr(pointer) => pointer,
        Slot::Value(_) => return Err(Error::AssignToArgument(name)),
    };
    if !binding.writable {
        return Err(Error::AssignToReadonly(name));
    }
    let (rhs, rhs_ty) = lower_expr(ctx, function, body, right, env)?;
    if rhs_ty != ty {
        return Err(Error::TypeMismatch);
    }
    let value = if let Some(op) = compound {
        let left_val = emit(function, body, Expression::Load { pointer })?;
        emit(
            function,
            body,
            Expression::Binary {
                op,
                left: left_val,
                right: rhs,
            },
        )?
    } else {
        rhs
    };
    body.push(Statement::Store { pointer, value }, Span::UNDEFINED);
    Ok((value, ty))
}

fn lower_lit(
    ctx: &mut Context,
    function: &mut Function,
    lit: &syn::ExprLit,
) -> Result<(Handle<Expression>, Handle<Type>), Error> {
    let (literal, ty) = match &lit.lit {
        syn::Lit::Float(f) => {
            if f.suffix() == "f64" {
                return Err(Error::UnsupportedType("f64".into()));
            }
            (
                Literal::F32(f.base10_parse().map_err(Error::from)?),
                ctx.intern_scalar(Scalar::F32),
            )
        }
        syn::Lit::Int(i) => match i.suffix() {
            "" | "i32" => (
                Literal::I32(i.base10_parse().map_err(Error::from)?),
                ctx.intern_scalar(Scalar::I32),
            ),
            "u32" => (
                Literal::U32(i.base10_parse().map_err(Error::from)?),
                ctx.intern_scalar(Scalar::U32),
            ),
            other => return Err(Error::UnsupportedType(other.into())),
        },
        syn::Lit::Bool(b) => (Literal::Bool(b.value()), ctx.intern_scalar(Scalar::BOOL)),
        _ => return Err(Error::UnsupportedExpr("literal".into())),
    };
    let handle = function
        .expressions
        .append(Expression::Literal(literal), Span::UNDEFINED);
    Ok((handle, ty))
}

fn bin_result_ty(
    ctx: &mut Context,
    op: BinaryOperator,
    left: Handle<Type>,
    right: Handle<Type>,
) -> Result<Handle<Type>, Error> {
    use BinaryOperator as Bo;
    if op == Bo::Multiply && left != right {
        return super::matrix::mul_result_ty(ctx, left, right);
    }
    if left != right {
        return Err(Error::TypeMismatch);
    }
    match op {
        Bo::Equal
        | Bo::NotEqual
        | Bo::Less
        | Bo::LessEqual
        | Bo::Greater
        | Bo::GreaterEqual => {
            if let Some((size, _)) = ctx.as_vector(left) {
                Ok(ctx.intern_vector(size, Scalar::BOOL))
            } else {
                Ok(ctx.intern_scalar(Scalar::BOOL))
            }
        }
        Bo::LogicalAnd | Bo::LogicalOr => {
            match ctx.module.types[left].inner {
                TypeInner::Scalar(s) if s == Scalar::BOOL => Ok(left),
                TypeInner::Vector { scalar, .. } if scalar == Scalar::BOOL => Ok(left),
                _ => Err(Error::TypeMismatch),
            }
        }
        _ => Ok(left),
    }
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

fn map_compound_op(op: &BinOp) -> Option<BinaryOperator> {
    Some(match op {
        BinOp::AddAssign(_) => BinaryOperator::Add,
        BinOp::SubAssign(_) => BinaryOperator::Subtract,
        BinOp::MulAssign(_) => BinaryOperator::Multiply,
        BinOp::DivAssign(_) => BinaryOperator::Divide,
        BinOp::RemAssign(_) => BinaryOperator::Modulo,
        BinOp::BitAndAssign(_) => BinaryOperator::And,
        BinOp::BitOrAssign(_) => BinaryOperator::InclusiveOr,
        BinOp::BitXorAssign(_) => BinaryOperator::ExclusiveOr,
        BinOp::ShlAssign(_) => BinaryOperator::ShiftLeft,
        BinOp::ShrAssign(_) => BinaryOperator::ShiftRight,
        _ => return None,
    })
}
