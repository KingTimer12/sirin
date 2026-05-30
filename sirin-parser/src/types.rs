#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    // primitives
    Int,
    Float,
    Str,
    Bool,
    Void,
    Nullable(Box<Type>),
    // explicit integer widths
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    // collections
    Array(Box<Type>),
    Vec(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Set(Box<Type>),
    // user-defined class/struct types
    Named(String),
    // anonymous struct literal { field: T, ... } — fields kept sorted by name (structural)
    Struct(Vec<(String, Type)>),
    // async channel
    Channel(Box<Type>),
}

impl Type {
    pub fn is_copy(&self) -> bool {
        matches!(
            self,
            Type::Int
                | Type::Float
                | Type::Bool
                | Type::U8
                | Type::U16
                | Type::U32
                | Type::U64
                | Type::I8
                | Type::I16
                | Type::I32
                | Type::I64
        )
    }

    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            Type::Int
                | Type::U8
                | Type::U16
                | Type::U32
                | Type::U64
                | Type::I8
                | Type::I16
                | Type::I32
                | Type::I64
        )
    }
}
