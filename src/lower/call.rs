use naga::{
    Block, Expression, Function, Handle, MathFunction, Span, Statement, Type,
};
use syn::Expr;

use super::emit::emit;
use super::env::Env;
use super::expr::lower_expr;
use super::parse_vec_ident;
use super::vector::lower_vec_ctor;
use super::Context;
use crate::Error;

pub(super) fn lower_call(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    call: &syn::ExprCall,
    env: &mut Env,
) -> Result<(Handle<Expression>, Handle<Type>), Error> {
    let name = match call.func.as_ref() {
        Expr::Path(path) if path.qself.is_none() && path.path.segments.len() == 1 => {
            path.path.segments[0].ident.to_string()
        }
        _ => return Err(Error::UnsupportedExpr("call".into())),
    };
    if parse_vec_ident(&name).is_some() {
        return lower_vec_ctor(ctx, function, body, call, env);
    }
    if let Some(spec) = math_spec(&name) {
        return lower_math(ctx, function, body, call, env, &name, spec);
    }
    lower_fn_call(ctx, function, body, call, env, &name)
}

struct MathSpec {
    fun: MathFunction,
    argc: usize,
    result: MathResult,
}

enum MathResult {
    SameAsFirst,
    ScalarOfFirst,
}

fn math_spec(name: &str) -> Option<MathSpec> {
    use MathFunction as Mf;
    use MathResult::*;
    let (fun, argc, result) = match name {
        "abs" | "sign" => (Mf::Abs, 1, SameAsFirst),
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
        _ => return None,
    };
    let fun = match name {
        "sign" => Mf::Sign,
        _ => fun,
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
) -> Result<(Handle<Expression>, Handle<Type>), Error> {
    if call.args.len() != spec.argc {
        return Err(Error::WrongArgCount(name.into()));
    }
    let mut args = Vec::new();
    let mut tys = Vec::new();
    for arg in &call.args {
        let (h, ty) = lower_expr(ctx, function, body, arg, env)?;
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
            } else {
                return Err(Error::TypeMismatch);
            }
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
) -> Result<(Handle<Expression>, Handle<Type>), Error> {
    let mut arg_values = Vec::new();
    let mut arg_tys = Vec::new();
    for arg in &call.args {
        let (h, ty) = lower_expr(ctx, function, body, arg, env)?;
        arg_values.push(h);
        arg_tys.push(ty);
    }

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

    if expected.len() != arg_tys.len() {
        return Err(Error::WrongArgCount(name.into()));
    }
    for (got, want) in arg_tys.iter().zip(expected.iter()) {
        if got != want {
            return Err(Error::TypeMismatch);
        }
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
