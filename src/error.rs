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
    #[error("`let` without initializer is not supported")]
    MissingLetInit,
    #[error("`if` used as a value needs an `else` branch")]
    IfExprMissingElse,
    #[error("block used as a value has no tail expression")]
    MissingBlockValue,
    #[error("type mismatch")]
    TypeMismatch,
    #[error("cannot assign to function argument `{0}`")]
    AssignToArgument(String),
    #[error("assignment target must be a local identifier")]
    InvalidAssignTarget,
    #[error("labeled loops are not supported")]
    LoopLabel,
    #[error("`break` with a value is not supported")]
    BreakValue,
    #[error("unsupported vector constructor `{0}`")]
    BadVecCtor(String),
    #[error("wrong number of components for vector constructor")]
    VecCtorArgs,
    #[error("unsupported swizzle `.{0}`")]
    UnsupportedSwizzle(String),
    #[error("vector component index out of range")]
    VecIndexRange,
    #[error("conflicting shader stage attributes")]
    ConflictingStage,
    #[error("unsupported binding `{0}`")]
    UnsupportedBinding(String),
    #[error("entry point argument `{0}` needs #[location] or #[builtin]")]
    MissingArgBinding(String),
    #[error("compute entry point needs #[workgroup_size]")]
    MissingWorkgroupSize,
    #[error("#[workgroup_size] is only valid on compute")]
    UnexpectedWorkgroupSize,
    #[error("entry point `{0}` is missing #[output(...)]")]
    MissingReturnBinding(String),
    #[error("unknown function `{0}`")]
    UnknownFunction(String),
    #[error("wrong number of arguments for `{0}`")]
    WrongArgCount(String),
    #[error("unsupported matrix constructor `{0}`")]
    BadMatCtor(String),
    #[error("wrong number of components for matrix constructor")]
    MatCtorArgs,
    #[error("resource `{0}` needs #[group] and #[binding]")]
    MissingResourceBinding(String),
    #[error("duplicate global `{0}`")]
    DuplicateGlobal(String),
    #[error("cannot assign to read-only global `{0}`")]
    AssignToReadonly(String),
    #[error("duplicate struct `{0}`")]
    DuplicateStruct(String),
    #[error("unknown struct `{0}`")]
    UnknownStruct(String),
    #[error("struct `{0}` has no fields")]
    EmptyStruct(String),
    #[error("duplicate field `{0}`")]
    DuplicateField(String),
    #[error("unknown field `{0}`")]
    UnknownField(String),
    #[error("missing field `{0}`")]
    MissingStructField(String),
    #[error("wrong number of fields for struct `{0}`")]
    StructFieldCount(String),
}
