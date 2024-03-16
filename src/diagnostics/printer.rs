use crate::text::SourceText;

use super::Diagnostics;

pub struct DiagnosticsPrinter<'a> {
    text: &'a SourceText,
    diagnostics: &'a [Diagnostics]
}