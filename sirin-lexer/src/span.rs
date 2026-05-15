use sirin_diagnostics::span::Span;

use crate::token::Tokens;

#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

pub type SpannedToken<'src> = Spanned<Tokens<'src>>;