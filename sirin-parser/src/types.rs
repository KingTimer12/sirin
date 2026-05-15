#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    Str,
    Bool,
    Void,
    Nullable(Box<Type>),
}

impl Type {
    /// Tipos Copy não geram move ao serem usados como RHS.
    pub fn is_copy(&self) -> bool {
        matches!(self, Type::Int | Type::Float | Type::Bool)
    }
}
