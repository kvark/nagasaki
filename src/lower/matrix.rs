use naga::{Block, Expression, Function, Handle, Type};
use syn::Expr;

use super::emit::emit;
use super::env::Env;
use super::expr::lower_expr;
use super::parse_mat_ident;
use super::Context;
use crate::Error;

pub(super) fn lower_mat_ctor(
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
    let (columns, rows, shorthand) =
        parse_mat_ident(&name).ok_or_else(|| Error::BadMatCtor(name.clone()))?;

    let mut components = Vec::new();
    let mut component_tys = Vec::new();
    for arg in &call.args {
        let (handle, ty) = lower_expr(ctx, function, body, arg, env)?;
        components.push(handle);
        component_tys.push(ty);
    }
    if components.is_empty() {
        return Err(Error::MatCtorArgs);
    }

    let scalar = if let Some(s) = shorthand {
        s
    } else if let Some(s) = ctx.as_scalar(component_tys[0]) {
        s
    } else if let Some((_, s)) = ctx.as_vector(component_tys[0]) {
        s
    } else if let Some((_, _, s)) = ctx.as_matrix(component_tys[0]) {
        s
    } else {
        return Err(Error::TypeMismatch);
    };

    let ty = ctx.intern_matrix(columns, rows, scalar);

    // Identity-ish copy: matN(m) where m already has that type.
    if components.len() == 1 && component_tys[0] == ty {
        return Ok((components[0], ty));
    }

    // Column vectors: matCxR(c0, c1, ...)
    if components.len() == columns as usize
        && component_tys
            .iter()
            .all(|&t| ctx.as_vector(t) == Some((rows, scalar)))
    {
        let handle = emit(
            function,
            body,
            Expression::Compose { ty, components },
        )?;
        return Ok((handle, ty));
    }

    // Flattened column-major scalars: mat2(a, b, c, d)
    let flat = (columns as usize) * (rows as usize);
    if components.len() == flat && component_tys.iter().all(|&t| ctx.as_scalar(t) == Some(scalar))
    {
        let mut columns_expr = Vec::new();
        let col_ty = ctx.intern_vector(rows, scalar);
        let col_width = rows as usize;
        for col in 0..columns as usize {
            let start = col * col_width;
            let slice = components[start..start + col_width].to_vec();
            let col_handle = emit(
                function,
                body,
                Expression::Compose {
                    ty: col_ty,
                    components: slice,
                },
            )?;
            columns_expr.push(col_handle);
        }
        let handle = emit(
            function,
            body,
            Expression::Compose {
                ty,
                components: columns_expr,
            },
        )?;
        return Ok((handle, ty));
    }

    Err(Error::MatCtorArgs)
}

/// Result type of `left * right` when one side is a matrix (Naga multiply rules).
pub(super) fn mul_result_ty(
    ctx: &mut Context,
    left: Handle<Type>,
    right: Handle<Type>,
) -> Result<Handle<Type>, Error> {
    let lmat = ctx.as_matrix(left);
    let rmat = ctx.as_matrix(right);
    let lvec = ctx.as_vector(left);
    let rvec = ctx.as_vector(right);
    let lsc = ctx.as_scalar(left);
    let rsc = ctx.as_scalar(right);

    match (lmat, lvec, lsc, rmat, rvec, rsc) {
        // matCxR * vecC → vecR
        (Some((columns, rows, s)), _, _, None, Some((size, s2)), _) if columns == size && s == s2 => {
            Ok(ctx.intern_vector(rows, s))
        }
        // vecR * matCxR → vecC
        (None, Some((size, s2)), _, Some((columns, rows, s)), _, _) if size == rows && s == s2 => {
            Ok(ctx.intern_vector(columns, s))
        }
        // matKxR * matCxK → matCxR
        (Some((c1, r1, s1)), _, _, Some((c2, r2, s2)), _, _) if c1 == r2 && s1 == s2 => {
            Ok(ctx.intern_matrix(c2, r1, s1))
        }
        // mat * scalar / scalar * mat
        (Some((_, _, s)), _, _, None, None, Some(s2)) if s == s2 => Ok(left),
        (None, None, Some(s2), Some((_, _, s)), _, _) if s == s2 => Ok(right),
        _ => Err(Error::TypeMismatch),
    }
}
