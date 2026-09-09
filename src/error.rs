#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Syn(#[from] syn::Error),
    #[error("unsupported item: {0}")]
    UnsupportedItem(String),
    #[error("unsupported type: {0}")]
    UnsupportedType(String),
    #[error("unsupported expression: {0}")]
    UnsupportedExpr(String),
    #[error("unsupported binary operator: {0}")]
    UnsupportedBinOp(String),
    #[error("unsupported statement: {0}")]
    UnsupportedStmt(String),
    #[error("unknown identifier: {0}")]
    UnknownIdent(String),
    #[error("function `{0}` has no return type")]
    MissingReturnType(String),
    #[error("receiver arguments are not supported")]
    Receiver,
    #[error("pattern parameters are not supported")]
    PatternParam,
}
