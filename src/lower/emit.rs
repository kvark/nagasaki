use naga::{proc::Emitter, Block, Expression, Function, Handle, Span};

use crate::Error;

pub(super) fn emit(
    function: &mut Function,
    body: &mut Block,
    expr: Expression,
) -> Result<Handle<Expression>, Error> {
    let mut emitter = Emitter::default();
    emitter.start(&function.expressions);
    let handle = function.expressions.append(expr, Span::UNDEFINED);
    body.extend(emitter.finish(&function.expressions));
    Ok(handle)
}

pub(super) fn item_kind(item: &syn::Item) -> String {
    use syn::Item;
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

pub(super) fn expr_kind(expr: &syn::Expr) -> String {
    use syn::Expr;
    match expr {
        Expr::Array(_) => "array literal",
        Expr::Assign(_) => "assignment",
        Expr::Async(_) => "async block",
        Expr::Await(_) => "await",
        Expr::Call(_) => "call",
        Expr::Cast(_) => "cast",
        Expr::Closure(_) => "closure",
        Expr::Const(_) => "const block",
        Expr::Field(_) => "field",
        Expr::ForLoop(_) => "`for` loop",
        Expr::If(_) => "if",
        Expr::Index(_) => "index",
        Expr::Let(_) => "`let` guard",
        Expr::Loop(_) => "loop",
        Expr::Macro(_) => "macro",
        Expr::Match(_) => "match",
        Expr::MethodCall(_) => "method",
        Expr::Range(_) => "range",
        Expr::Reference(_) => "reference",
        Expr::Repeat(_) => "array repeat",
        Expr::Struct(_) => "struct literal",
        Expr::Try(_) => "`?`",
        Expr::Tuple(_) => "tuple",
        Expr::Unsafe(_) => "unsafe block",
        Expr::While(_) => "while",
        Expr::Yield(_) => "yield",
        _ => "expression",
    }
    .into()
}
