use naga::{
    Block, Expression, Function, Handle, Scalar, SwizzleComponent, Type, VectorSize,
};
use syn::Expr;

use super::emit::emit;
use super::env::Env;
use super::expr::lower_expr;
use super::parse_vec_ident;
use super::Context;
use crate::Error;

pub(super) fn splat_mix(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    left: &mut Handle<Expression>,
    left_ty: &mut Handle<Type>,
    right: &mut Handle<Expression>,
    right_ty: &mut Handle<Type>,
) -> Result<(), Error> {
    match (ctx.as_vector(*left_ty), ctx.as_scalar(*left_ty), ctx.as_vector(*right_ty), ctx.as_scalar(*right_ty))
    {
        (Some((size, scalar)), _, None, Some(s)) if s == scalar => {
            *right = emit(function, body, Expression::Splat { size, value: *right })?;
            *right_ty = ctx.intern_vector(size, scalar);
        }
        (None, Some(s), Some((size, scalar)), _) if s == scalar => {
            *left = emit(function, body, Expression::Splat { size, value: *left })?;
            *left_ty = ctx.intern_vector(size, scalar);
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn lower_vec_ctor(
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
    let (size, shorthand) = parse_vec_ident(&name)
        .ok_or_else(|| Error::BadVecCtor(name.clone()))?;

    let mut components = Vec::new();
    let mut component_tys = Vec::new();
    for arg in &call.args {
        let (handle, ty) = lower_expr(ctx, function, body, arg, env)?;
        components.push(handle);
        component_tys.push(ty);
    }
    if components.is_empty() {
        return Err(Error::VecCtorArgs);
    }

    let scalar = if let Some(s) = shorthand {
        s
    } else if let Some(s) = ctx.as_scalar(component_tys[0]) {
        s
    } else if let Some((_, s)) = ctx.as_vector(component_tys[0]) {
        s
    } else {
        return Err(Error::TypeMismatch);
    };

    let mut width = 0u32;
    for &ty in &component_tys {
        if let Some(s) = ctx.as_scalar(ty) {
            if s != scalar {
                return Err(Error::TypeMismatch);
            }
            width += 1;
        } else if let Some((comp_size, s)) = ctx.as_vector(ty) {
            if s != scalar {
                return Err(Error::TypeMismatch);
            }
            width += comp_size as u32;
        } else {
            return Err(Error::TypeMismatch);
        }
    }

    let ty = ctx.intern_vector(size, scalar);
    if components.len() == 1 && ctx.as_scalar(component_tys[0]).is_some() {
        let handle = emit(
            function,
            body,
            Expression::Splat {
                size,
                value: components[0],
            },
        )?;
        return Ok((handle, ty));
    }
    if components.len() == 1 && ctx.as_vector(component_tys[0]) == Some((size, scalar)) {
        return Ok((components[0], ty));
    }
    if width != size as u32 {
        return Err(Error::VecCtorArgs);
    }
    let handle = emit(
        function,
        body,
        Expression::Compose { ty, components },
    )?;
    Ok((handle, ty))
}

pub(super) fn lower_field(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    field: &syn::ExprField,
    env: &mut Env,
) -> Result<(Handle<Expression>, Handle<Type>), Error> {
    let member = match &field.member {
        syn::Member::Named(ident) => ident.to_string(),
        syn::Member::Unnamed(_) => return Err(Error::UnsupportedExpr("tuple field".into())),
    };
    let (base, base_ty) = lower_expr(ctx, function, body, &field.base, env)?;
    if ctx.as_struct(base_ty).is_some() {
        return super::structure::lower_struct_field(
            ctx, function, body, base, base_ty, &member,
        );
    }
    let (vec_size, scalar) = ctx
        .as_vector(base_ty)
        .ok_or_else(|| Error::UnsupportedExpr("field".into()))?;
    let letters: Vec<char> = member.chars().collect();
    if letters.is_empty() || letters.len() > 4 || !letters.iter().all(|c| matches!(c, 'x' | 'y' | 'z' | 'w'))
    {
        return Err(Error::UnsupportedSwizzle(member));
    }
    let max = vec_size as u32;
    let mut pattern = [SwizzleComponent::X; 4];
    for (i, ch) in letters.iter().enumerate() {
        let index = match ch {
            'x' => 0,
            'y' => 1,
            'z' => 2,
            'w' => 3,
            _ => unreachable!(),
        };
        if index >= max {
            return Err(Error::UnsupportedSwizzle(member));
        }
        pattern[i] = match index {
            0 => SwizzleComponent::X,
            1 => SwizzleComponent::Y,
            2 => SwizzleComponent::Z,
            _ => SwizzleComponent::W,
        };
    }
    if letters.len() == 1 {
        let handle = emit(
            function,
            body,
            Expression::AccessIndex {
                base,
                index: pattern[0] as u32,
            },
        )?;
        return Ok((handle, ctx.intern_scalar(scalar)));
    }
    let out_size = match letters.len() {
        2 => VectorSize::Bi,
        3 => VectorSize::Tri,
        4 => VectorSize::Quad,
        _ => return Err(Error::UnsupportedSwizzle(member)),
    };
    let handle = emit(
        function,
        body,
        Expression::Swizzle {
            size: out_size,
            vector: base,
            pattern,
        },
    )?;
    Ok((handle, ctx.intern_vector(out_size, scalar)))
}

pub(super) fn lower_index(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    index: &syn::ExprIndex,
    env: &mut Env,
) -> Result<(Handle<Expression>, Handle<Type>), Error> {
    let (base, base_ty) = lower_expr(ctx, function, body, &index.expr, env)?;
    let (bound, result_ty) = if let Some((vec_size, scalar)) = ctx.as_vector(base_ty) {
        (vec_size as u32, ctx.intern_scalar(scalar))
    } else if let Some((columns, rows, scalar)) = ctx.as_matrix(base_ty) {
        (columns as u32, ctx.intern_vector(rows, scalar))
    } else {
        return Err(Error::UnsupportedExpr("index".into()));
    };
    if let Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Int(int),
        ..
    }) = index.index.as_ref()
    {
        let idx: u32 = int.base10_parse().map_err(Error::from)?;
        if idx >= bound {
            return Err(Error::VecIndexRange);
        }
        let handle = emit(
            function,
            body,
            Expression::AccessIndex { base, index: idx },
        )?;
        return Ok((handle, result_ty));
    }
    let (idx_expr, idx_ty) = lower_expr(ctx, function, body, &index.index, env)?;
    match ctx.as_scalar(idx_ty) {
        Some(s) if s == Scalar::I32 || s == Scalar::U32 => {}
        _ => return Err(Error::TypeMismatch),
    }
    let handle = emit(
        function,
        body,
        Expression::Access {
            base,
            index: idx_expr,
        },
    )?;
    Ok((handle, result_ty))
}
