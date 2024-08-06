use std::cmp;

use termion::color::{Fg, Red, Reset};

use crate::text::SourceText;

use super::Diagnostics;

const PREFIX_LENGTH: usize = 8;

pub struct DiagnosticsPrinter<'a> {
    text: &'a SourceText,
    diagnostics: &'a [Diagnostics]
}

impl <'a> DiagnosticsPrinter<'a> {
    pub fn new(text: &'a SourceText, diagnostics: &'a [Diagnostics]) -> Self {
        Self { text, diagnostics }
    }

    pub fn stringify_diagnostic(&self, diagnostic: &Diagnostics) -> String {
        let line_index = self.text.line_index(diagnostic.span.start);
        let line = self.text.get_line(line_index);
        let line_start = self.text.line_start(line_index);

        let col = diagnostic.span.start - line_start;
        let (prefix, span, suffix) = self.get_text_span(diagnostic, line, col);

        let indent = cmp::min(PREFIX_LENGTH, col);
        let (arrow_pointers, arrow_line) = Self::format_arrow(diagnostic, indent);
        let error_message = format!("{:indent$}+-- {}", "", diagnostic.message, indent = indent);
        format!("{}{}{}{}{}\n{}\n{}\n{}", prefix, Fg(Red), span, Fg(Reset), suffix, arrow_pointers, arrow_line, error_message)
    }

    fn format_arrow(diagnostic: &Diagnostics, indent: usize) -> (String, String) {
        let arrow_pointers = format!("{:indent$}{}", "", std::iter::repeat('^').take(diagnostic.span.length()).collect::<String>(), indent = indent);
        let arrow_line = format!("{:indent$}|", "", indent = indent);
        (arrow_pointers, arrow_line)
    }

    fn get_text_span(&'a self, diagnostic: &Diagnostics, line: &'a str, column: usize) -> (&'a str, &'a str, &'a str) {
        let prefix_start = cmp::max(0, column as isize - PREFIX_LENGTH as isize) as usize;
        let prefix_end = column;
        let prefix = &line[prefix_start..prefix_end];

        let suffix_start = cmp::min(column + diagnostic.span.length(), line.len());
        let suffix_end = cmp::min(suffix_start + PREFIX_LENGTH, line.len());
        let suffix = &line[suffix_start..suffix_end];

        let span = &line[prefix_end..suffix_start];

        (prefix, span, suffix)
    }

    pub fn print(&self) {
        for diagnostic in self.diagnostics {
            println!("{}", self.stringify_diagnostic(diagnostic))
        }
    }
}