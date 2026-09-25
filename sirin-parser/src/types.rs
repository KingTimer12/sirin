#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    // primitives
    Int,
    Float,
    Str,
    Bool,
    Void,
    Nullable(Box<Type>),
    // fallible result `T!` — Ok(T) | Err(str); error type fixed to str for now
    Try(Box<Type>),
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
    // first-class function value: `fn(T1, T2) -> R`
    Func(Vec<Type>, Box<Type>),
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

/// Renders a type in Sirin source syntax (`Map[str, int]`, `str?`), for error messages.
impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Str => write!(f, "str"),
            Type::Bool => write!(f, "bool"),
            Type::Void => write!(f, "void"),
            Type::Nullable(t) => write!(f, "{t}?"),
            Type::Try(t) => write!(f, "{t}!"),
            Type::U8 => write!(f, "u8"),
            Type::U16 => write!(f, "u16"),
            Type::U32 => write!(f, "u32"),
            Type::U64 => write!(f, "u64"),
            Type::I8 => write!(f, "i8"),
            Type::I16 => write!(f, "i16"),
            Type::I32 => write!(f, "i32"),
            Type::I64 => write!(f, "i64"),
            Type::Array(t) => write!(f, "Array[{t}]"),
            Type::Vec(t) => write!(f, "Vec[{t}]"),
            Type::Map(k, v) => write!(f, "Map[{k}, {v}]"),
            Type::Set(t) => write!(f, "Set[{t}]"),
            Type::Named(name) => write!(f, "{name}"),
            Type::Struct(fields) => {
                write!(f, "{{ ")?;
                for (i, (name, ty)) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{name}: {ty}")?;
                }
                write!(f, " }}")
            }
            Type::Channel(t) => write!(f, "Channel[{t}]"),
            Type::Func(args, ret) => {
                write!(f, "fn(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{a}")?;
                }
                write!(f, ") -> {ret}")
            }
        }
    }
}
