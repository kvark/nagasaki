use naga::{Expression, Handle, Type};

pub(crate) struct Binding {
    pub name: String,
    pub slot: Slot,
    pub ty: Handle<Type>,
}

pub(crate) enum Slot {
    /// Already a value (`FunctionArgument`).
    Value(Handle<Expression>),
    /// Pointer to a `LocalVariable`.
    Ptr(Handle<Expression>),
}

#[derive(Default)]
pub(crate) struct Env {
    bindings: Vec<Binding>,
    scopes: Vec<usize>,
}

impl Env {
    pub fn push_scope(&mut self) {
        self.scopes.push(self.bindings.len());
    }

    pub fn pop_scope(&mut self) {
        let start = self.scopes.pop().unwrap_or(0);
        self.bindings.truncate(start);
    }

    pub fn push(&mut self, name: String, slot: Slot, ty: Handle<Type>) {
        self.bindings.push(Binding { name, slot, ty });
    }

    pub fn lookup(&self, name: &str) -> Option<&Binding> {
        self.bindings.iter().rev().find(|b| b.name == name)
    }
}
