use naga::{Block, Expression, Function, Handle, LocalVariable, Span, Statement, Type};
use syn::{Block as SynBlock, Expr, Local, Pat, Stmt, Type as SynType};

use super::emit::emit;
use super::env::{Env, Slot};
use super::expr::lower_expr;
use super::Context;
use crate::Error;

fn is_stmt_like(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Loop(_) | Expr::While(_) | Expr::Break(_) | Expr::Continue(_) | Expr::ForLoop(_)
    )
}

pub(super) fn lower_block(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    block: &SynBlock,
    env: &mut Env,
) -> Result<Option<(Handle<Expression>, Handle<Type>)>, Error> {
    let mut tail = None;
    for (i, stmt) in block.stmts.iter().enumerate() {
        let last = i + 1 == block.stmts.len();
        match stmt {
            Stmt::Local(local) => {
                lower_local(ctx, function, body, local, env)?;
                tail = None;
            }
            Stmt::Expr(Expr::Return(ret), _) => {
                let value = match ret.expr.as_deref() {
                    Some(expr) => Some(lower_expr(ctx, function, body, expr, env)?.0),
                    None => None,
                };
                body.push(Statement::Return { value }, Span::UNDEFINED);
                tail = None;
            }
            Stmt::Expr(expr, semi) => {
                if last && semi.is_none() && !is_stmt_like(expr) {
                    tail = Some(lower_expr(ctx, function, body, expr, env)?);
                } else {
                    lower_stmt_expr(ctx, function, body, expr, env)?;
                    tail = None;
                }
            }
            Stmt::Item(_) => return Err(Error::UnsupportedStmt("item in block".into())),
            Stmt::Macro(_) => return Err(Error::UnsupportedStmt("macro".into())),
        }
    }
    Ok(tail)
}

fn lower_stmt_expr(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    expr: &Expr,
    env: &mut Env,
) -> Result<(), Error> {
    match expr {
        Expr::If(if_expr) => lower_if_stmt(ctx, function, body, if_expr, env),
        Expr::While(while_expr) => lower_while(ctx, function, body, while_expr, env),
        Expr::Loop(loop_expr) => lower_loop(ctx, function, body, loop_expr, env),
        Expr::Break(brk) => lower_break(body, brk),
        Expr::Continue(cont) => lower_continue(body, cont),
        Expr::Block(b) => {
            env.push_scope();
            let _ = lower_block(ctx, function, body, &b.block, env)?;
            env.pop_scope();
            Ok(())
        }
        Expr::Paren(inner) => lower_stmt_expr(ctx, function, body, &inner.expr, env),
        Expr::Group(inner) => lower_stmt_expr(ctx, function, body, &inner.expr, env),
        other => {
            let _ = lower_expr(ctx, function, body, other, env)?;
            Ok(())
        }
    }
}

fn lower_while(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    while_expr: &syn::ExprWhile,
    env: &mut Env,
) -> Result<(), Error> {
    if while_expr.label.is_some() {
        return Err(Error::LoopLabel);
    }
    let mut loop_body = Block::new();
    env.push_scope();
    let (condition, _) = lower_expr(ctx, function, &mut loop_body, &while_expr.cond, env)?;
    let mut accept = Block::new();
    let _ = lower_block(ctx, function, &mut accept, &while_expr.body, env)?;
    let mut reject = Block::new();
    reject.push(Statement::Break, Span::UNDEFINED);
    loop_body.push(
        Statement::If {
            condition,
            accept,
            reject,
        },
        Span::UNDEFINED,
    );
    env.pop_scope();
    body.push(
        Statement::Loop {
            body: loop_body,
            continuing: Block::new(),
            break_if: None,
        },
        Span::UNDEFINED,
    );
    Ok(())
}

fn lower_loop(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    loop_expr: &syn::ExprLoop,
    env: &mut Env,
) -> Result<(), Error> {
    if loop_expr.label.is_some() {
        return Err(Error::LoopLabel);
    }
    let mut loop_body = Block::new();
    env.push_scope();
    let _ = lower_block(ctx, function, &mut loop_body, &loop_expr.body, env)?;
    env.pop_scope();
    body.push(
        Statement::Loop {
            body: loop_body,
            continuing: Block::new(),
            break_if: None,
        },
        Span::UNDEFINED,
    );
    Ok(())
}

fn lower_break(body: &mut Block, brk: &syn::ExprBreak) -> Result<(), Error> {
    if brk.label.is_some() {
        return Err(Error::LoopLabel);
    }
    if brk.expr.is_some() {
        return Err(Error::BreakValue);
    }
    body.push(Statement::Break, Span::UNDEFINED);
    Ok(())
}

fn lower_continue(body: &mut Block, cont: &syn::ExprContinue) -> Result<(), Error> {
    if cont.label.is_some() {
        return Err(Error::LoopLabel);
    }
    body.push(Statement::Continue, Span::UNDEFINED);
    Ok(())
}

fn lower_local(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    local: &Local,
    env: &mut Env,
) -> Result<(), Error> {
    let init = local.init.as_ref().ok_or(Error::MissingLetInit)?;
    if init.diverge.is_some() {
        return Err(Error::UnsupportedStmt("let else".into()));
    }
    let (name, annot) = bind_ident_pat(&local.pat)?;
    let (value, value_ty) = lower_expr(ctx, function, body, &init.expr, env)?;
    let ty = match annot {
        Some(ty) => {
            let ty = ctx.lower_type(ty)?;
            if ty != value_ty {
                return Err(Error::TypeMismatch);
            }
            ty
        }
        None => value_ty,
    };
    let local_var = function.local_variables.append(
        LocalVariable {
            name: Some(name.clone()),
            ty,
            init: None,
        },
        Span::UNDEFINED,
    );
    let pointer = function
        .expressions
        .append(Expression::LocalVariable(local_var), Span::UNDEFINED);
    body.push(Statement::Store { pointer, value }, Span::UNDEFINED);
    env.push(name, Slot::Ptr(pointer), ty);
    Ok(())
}

fn bind_ident_pat(pat: &Pat) -> Result<(String, Option<&SynType>), Error> {
    match pat {
        Pat::Ident(ident) if ident.by_ref.is_none() => Ok((ident.ident.to_string(), None)),
        Pat::Type(pat_ty) => {
            let (name, _) = bind_ident_pat(&pat_ty.pat)?;
            Ok((name, Some(&*pat_ty.ty)))
        }
        _ => Err(Error::PatternParam),
    }
}

fn lower_if_stmt(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    if_expr: &syn::ExprIf,
    env: &mut Env,
) -> Result<(), Error> {
    let (condition, _) = lower_expr(ctx, function, body, &if_expr.cond, env)?;
    let mut accept = Block::new();
    env.push_scope();
    let _ = lower_block(ctx, function, &mut accept, &if_expr.then_branch, env)?;
    env.pop_scope();
    let mut reject = Block::new();
    if let Some((_, else_expr)) = &if_expr.else_branch {
        env.push_scope();
        match else_expr.as_ref() {
            Expr::Block(b) => {
                let _ = lower_block(ctx, function, &mut reject, &b.block, env)?;
            }
            Expr::If(inner) => lower_if_stmt(ctx, function, &mut reject, inner, env)?,
            other => {
                let _ = lower_expr(ctx, function, &mut reject, other, env)?;
            }
        }
        env.pop_scope();
    }
    body.push(
        Statement::If {
            condition,
            accept,
            reject,
        },
        Span::UNDEFINED,
    );
    Ok(())
}

pub(super) fn lower_if_expr(
    ctx: &mut Context,
    function: &mut Function,
    body: &mut Block,
    if_expr: &syn::ExprIf,
    env: &mut Env,
) -> Result<(Handle<Expression>, Handle<Type>), Error> {
    let else_expr = if_expr
        .else_branch
        .as_ref()
        .map(|(_, e)| e.as_ref())
        .ok_or(Error::IfExprMissingElse)?;
    let (condition, _) = lower_expr(ctx, function, body, &if_expr.cond, env)?;
    let mut accept = Block::new();
    env.push_scope();
    let (then_val, then_ty) = lower_block(ctx, function, &mut accept, &if_expr.then_branch, env)?
        .ok_or(Error::MissingBlockValue)?;
    env.pop_scope();
    let mut reject = Block::new();
    env.push_scope();
    let (else_val, else_ty) = match else_expr {
        Expr::Block(b) => lower_block(ctx, function, &mut reject, &b.block, env)?
            .ok_or(Error::MissingBlockValue)?,
        Expr::If(inner) => lower_if_expr(ctx, function, &mut reject, inner, env)?,
        other => lower_expr(ctx, function, &mut reject, other, env)?,
    };
    env.pop_scope();
    if then_ty != else_ty {
        return Err(Error::TypeMismatch);
    }
    let ty = then_ty;
    let local = function.local_variables.append(
        LocalVariable {
            name: None,
            ty,
            init: None,
        },
        Span::UNDEFINED,
    );
    let pointer = function
        .expressions
        .append(Expression::LocalVariable(local), Span::UNDEFINED);
    accept.push(
        Statement::Store {
            pointer,
            value: then_val,
        },
        Span::UNDEFINED,
    );
    reject.push(
        Statement::Store {
            pointer,
            value: else_val,
        },
        Span::UNDEFINED,
    );
    body.push(
        Statement::If {
            condition,
            accept,
            reject,
        },
        Span::UNDEFINED,
    );
    let loaded = emit(function, body, Expression::Load { pointer })?;
    Ok((loaded, ty))
}
