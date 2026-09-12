use naga::{Block, Expression, Function, MathFunction, Span, Statement};
use syn::Expr;

use super::emit::emit;
use super::env::Env;
use super::expr::{lower_expr, lower_expr_hinted};
use super::matrix::lower_mat_ctor;
use super::parse_mat_ident;
use super::parse_vec_ident;
use super::texture;
use super::vector::lower_vec_ctor;
use super::{Context, Shape, Typed};
use crate::Error;

/// The name and kind of a texture builtin that writes instead of producing.
pub(super) fn statement_builtin(call: &syn::ExprCall) -> Option<(String, texture::TextureOp)> {
    let Expr::Path(path) = call.func.as_ref() else {
        return None;
    };
    if path.qself.is_some() || path.path.segments.len() != 1 {
        return None;
    }
    let name = path.path.segments[0].ident.to_string();
    let op = texture::texture_builtin(&name)?;
    op.is_statement().then_some((name, op))
}

pub(super) fn lower_call(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    call: &syn::ExprCall,
    env: &mut Env,
) -> Result<Typed, Error> {
    let name = match call.func.as_ref() {
        Expr::Path(path) if path.qself.is_none() && path.path.segments.len() == 1 => {
            path.path.segments[0].ident.to_string()
        }
        _ => return Err(Error::UnsupportedExpr("call".into())),
    };
    if parse_vec_ident(&name).is_some() {
        return lower_vec_ctor(ctx, function, body, call, env);
    }
    if parse_mat_ident(&name).is_some() {
        return lower_mat_ctor(ctx, function, body, call, env);
    }
    // A function the user declared wins over a builtin of the same name.
    // Resolving the other way round would silently call the builtin while
    // Naga renamed the user's function out of the way.
    let declared = ctx
        .module
        .functions
        .iter()
        .any(|(_, f)| f.name.as_deref() == Some(name.as_str()));
    if !declared {
        if name == "select" {
            return lower_select(ctx, function, body, call, env);
        }
        if let Some(op) = texture::texture_builtin(&name) {
            if op.is_statement() {
                return Err(Error::ValueFromStatement(name));
            }
            return texture::lower_texture_call(ctx, function, body, call, env, &name, op);
        }
        if let Some(spec) = math_spec(&name) {
            return lower_math(ctx, function, body, call, env, &name, spec);
        }
    }
    lower_fn_call(ctx, function, body, call, env, &name)
}

/// `select(reject, accept, condition)`, in WGSL's argument order: the value
/// picked when the condition holds comes second.
fn lower_select(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    call: &syn::ExprCall,
    env: &mut Env,
) -> Result<Typed, Error> {
    if call.args.len() != 3 {
        return Err(Error::WrongArgCount("select".into()));
    }
    let (reject, ty) = lower_expr_hinted(ctx, function, body, &call.args[0], env, None)?;
    let hint = ctx.shape(ty).int_hint();
    let (accept, accept_ty) = lower_expr_hinted(ctx, function, body, &call.args[1], env, hint)?;
    if accept_ty != ty {
        return Err(Error::TypeMismatch);
    }
    let (condition, cond_ty) = lower_expr(ctx, function, body, &call.args[2], env)?;
    // A scalar condition picks one whole value; a vector one picks per lane, so
    // it has to line up with the operands.
    let ok = match (ctx.shape(cond_ty), ctx.shape(ty)) {
        (Shape::Scalar(s), _) => s == naga::Scalar::BOOL,
        (Shape::Vector(size, s), Shape::Vector(operand, _)) => {
            s == naga::Scalar::BOOL && size == operand
        }
        _ => false,
    };
    if !ok {
        return Err(Error::TypeMismatch);
    }
    let handle = emit(
        function,
        body,
        Expression::Select {
            condition,
            accept,
            reject,
        },
    )?;
    Ok((handle, ty))
}

struct MathSpec {
    fun: MathFunction,
    argc: usize,
    result: MathResult,
}

enum MathResult {
    SameAsFirst,
    ScalarOfFirst,
    Transpose,
}

fn math_spec(name: &str) -> Option<MathSpec> {
    use MathFunction as Mf;
    use MathResult::*;
    let (fun, argc, result) = match name {
        "abs" => (Mf::Abs, 1, SameAsFirst),
        "sign" => (Mf::Sign, 1, SameAsFirst),
        "saturate" => (Mf::Saturate, 1, SameAsFirst),
        "sin" => (Mf::Sin, 1, SameAsFirst),
        "cos" => (Mf::Cos, 1, SameAsFirst),
        "tan" => (Mf::Tan, 1, SameAsFirst),
        "asin" => (Mf::Asin, 1, SameAsFirst),
        "acos" => (Mf::Acos, 1, SameAsFirst),
        "atan" => (Mf::Atan, 1, SameAsFirst),
        "floor" => (Mf::Floor, 1, SameAsFirst),
        "ceil" => (Mf::Ceil, 1, SameAsFirst),
        "round" => (Mf::Round, 1, SameAsFirst),
        "fract" => (Mf::Fract, 1, SameAsFirst),
        "sqrt" => (Mf::Sqrt, 1, SameAsFirst),
        "inverse_sqrt" | "inversesqrt" => (Mf::InverseSqrt, 1, SameAsFirst),
        "normalize" => (Mf::Normalize, 1, SameAsFirst),
        "exp" => (Mf::Exp, 1, SameAsFirst),
        "exp2" => (Mf::Exp2, 1, SameAsFirst),
        "log" => (Mf::Log, 1, SameAsFirst),
        "log2" => (Mf::Log2, 1, SameAsFirst),
        "min" => (Mf::Min, 2, SameAsFirst),
        "max" => (Mf::Max, 2, SameAsFirst),
        "pow" => (Mf::Pow, 2, SameAsFirst),
        "atan2" => (Mf::Atan2, 2, SameAsFirst),
        "reflect" => (Mf::Reflect, 2, SameAsFirst),
        "cross" => (Mf::Cross, 2, SameAsFirst),
        "clamp" => (Mf::Clamp, 3, SameAsFirst),
        "mix" => (Mf::Mix, 3, SameAsFirst),
        "smoothstep" => (Mf::SmoothStep, 3, SameAsFirst),
        "fma" => (Mf::Fma, 3, SameAsFirst),
        "face_forward" | "faceforward" => (Mf::FaceForward, 3, SameAsFirst),
        "dot" => (Mf::Dot, 2, ScalarOfFirst),
        "distance" => (Mf::Distance, 2, ScalarOfFirst),
        "length" => (Mf::Length, 1, ScalarOfFirst),
        "transpose" => (Mf::Transpose, 1, Transpose),
        "determinant" => (Mf::Determinant, 1, ScalarOfFirst),
        _ => return None,
    };
    Some(MathSpec { fun, argc, result })
}

fn lower_math(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    call: &syn::ExprCall,
    env: &mut Env,
    name: &str,
    spec: MathSpec,
) -> Result<Typed, Error> {
    if call.args.len() != spec.argc {
        return Err(Error::WrongArgCount(name.into()));
    }
    // `clamp(n, 0, 1)`: the first argument fixes the type, the rest follow it.
    let mut hint = None;
    let mut args = Vec::new();
    let mut tys = Vec::new();
    for arg in &call.args {
        let (h, ty) = lower_expr_hinted(ctx, function, body, arg, env, hint)?;
        hint = hint.or_else(|| ctx.shape(ty).int_hint());
        args.push(h);
        tys.push(ty);
    }
    let result_ty = match spec.result {
        MathResult::SameAsFirst => tys[0],
        MathResult::ScalarOfFirst => {
            if let Some(s) = ctx.as_scalar(tys[0]) {
                ctx.intern_scalar(s)
            } else if let Some((_, s)) = ctx.as_vector(tys[0]) {
                ctx.intern_scalar(s)
            } else if let Some((columns, rows, s)) = ctx.as_matrix(tys[0]) {
                if columns != rows {
                    return Err(Error::TypeMismatch);
                }
                ctx.intern_scalar(s)
            } else {
                return Err(Error::TypeMismatch);
            }
        }
        MathResult::Transpose => {
            let (columns, rows, s) = ctx.as_matrix(tys[0]).ok_or(Error::TypeMismatch)?;
            ctx.intern_matrix(rows, columns, s)
        }
    };
    let handle = emit(
        function,
        body,
        Expression::Math {
            fun: spec.fun,
            arg: args[0],
            arg1: args.get(1).copied(),
            arg2: args.get(2).copied(),
            arg3: args.get(3).copied(),
        },
    )?;
    Ok((handle, result_ty))
}

fn lower_fn_call(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    call: &syn::ExprCall,
    env: &mut Env,
    name: &str,
) -> Result<Typed, Error> {
    let (callee, expected, ret_ty) = {
        let found = ctx
            .module
            .functions
            .iter()
            .find(|(_, f)| f.name.as_deref() == Some(name));
        let (handle, func) = found.ok_or_else(|| Error::UnknownFunction(name.into()))?;
        let expected: Vec<_> = func.arguments.iter().map(|a| a.ty).collect();
        let ret = func
            .result
            .as_ref()
            .map(|r| r.ty)
            .ok_or_else(|| Error::MissingReturnType(name.into()))?;
        (handle, expected, ret)
    };

    if expected.len() != call.args.len() {
        return Err(Error::WrongArgCount(name.into()));
    }
    let mut arg_values = Vec::new();
    for (arg, &want) in call.args.iter().zip(expected.iter()) {
        let hint = ctx.shape(want).int_hint();
        let (h, ty) = lower_expr_hinted(ctx, function, body, arg, env, hint)?;
        if ty != want {
            return Err(Error::TypeMismatch);
        }
        arg_values.push(h);
    }

    let result = function
        .expressions
        .append(Expression::CallResult(callee), Span::UNDEFINED);
    body.push(
        Statement::Call {
            function: callee,
            arguments: arg_values,
            result: Some(result),
        },
        Span::UNDEFINED,
    );
    Ok((result, ret_ty))
}
