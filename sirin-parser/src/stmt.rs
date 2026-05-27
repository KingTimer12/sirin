use crate::{expr::Expr, span::Spanned, types::Type};

#[derive(Debug, Clone)]
pub enum ImplTarget<'a> {
    Named(&'a str),
    Int,
    Float,
    Str,
    Bool,
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
}

#[derive(Debug)]
pub struct ClassField<'a> {
    pub name: Spanned<&'a str>,
    pub ty: Type,
    pub mutable: bool,
    pub private: bool,
}

#[derive(Debug)]
pub struct InterfaceMethod<'a> {
    pub name: Spanned<&'a str>,
    pub args: Vec<(Spanned<&'a str>, Type)>,
    pub return_type: Option<Type>,
}

#[derive(Debug)]
pub enum Stmt<'a> {
    Let {
        name: Spanned<&'a str>,
        ty: Option<Type>,
        rhs: Spanned<Expr<'a>>,
    },
    CopyLet {
        name: Spanned<&'a str>,
        ty: Option<Type>,
        rhs: Spanned<Expr<'a>>,
    },
    Fn {
        async_: bool,
        name: Spanned<&'a str>,
        args: Vec<(Spanned<&'a str>, Type)>,
        return_type: Option<Type>,
        body: Vec<Spanned<Stmt<'a>>>,
    },
    Spawn {
        body: Vec<Spanned<Stmt<'a>>>,
    },
    Return {
        value: Option<Box<Spanned<Expr<'a>>>>,
        cond: Option<Box<Spanned<Expr<'a>>>>,
    },
    If {
        cond: Box<Spanned<Expr<'a>>>,
        then: Vec<Spanned<Stmt<'a>>>,
        else_: Option<Vec<Spanned<Stmt<'a>>>>,
    },
    Expr(Spanned<Expr<'a>>),
    Class {
        name: Spanned<&'a str>,
        abstract_: bool,
        extends: Option<Spanned<&'a str>>,
        is_: Vec<Spanned<&'a str>>,
        fields: Vec<ClassField<'a>>,
        methods: Vec<Spanned<Stmt<'a>>>,
    },
    Interface {
        name: Spanned<&'a str>,
        methods: Vec<InterfaceMethod<'a>>,
    },
    Impl {
        target: ImplTarget<'a>,
        methods: Vec<Spanned<Stmt<'a>>>,
    },
    Init {
        args: Vec<(Spanned<&'a str>, Type)>,
        body: Vec<Spanned<Stmt<'a>>>,
    },
    Default {
        body: Vec<Spanned<Stmt<'a>>>,
    },
    AbstractFn {
        name: Spanned<&'a str>,
        args: Vec<(Spanned<&'a str>, Type)>,
        return_type: Option<Type>,
    },
    Use {
        path: Vec<&'a str>,
    },
}
