pub mod span;

use std::ops::Range;

use ariadne::{Color, Config, IndexType, Label, Report, ReportKind, Source};

/// A compiler error with enough context to render it for a human: a short
/// title, the source range it points at, a label explaining what is wrong
/// there, and an optional hint on how to fix it.
///
/// Producing a `Diagnostic` never prints anything — callers decide where it
/// goes (stderr for the CLI, an LSP message for the language server).
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub title: String,
    pub span: Range<usize>,
    pub label: Option<String>,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn error(title: impl Into<String>, span: Range<usize>) -> Self {
        Self { title: title.into(), span, label: None, help: None }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// One-paragraph plain-text form, for places that cannot draw source
    /// snippets (editor popups, logs).
    pub fn message(&self) -> String {
        let mut msg = self.title.clone();
        if let Some(label) = &self.label {
            msg.push_str(": ");
            msg.push_str(label);
        }
        if let Some(help) = &self.help {
            msg.push_str("\nhelp: ");
            msg.push_str(help);
        }
        msg
    }

    /// Print the diagnostic to stderr with the offending source highlighted.
    pub fn eprint(&self, file: &str, src: &str) {
        // Clamp so an end-of-input span never points past the source.
        let start = self.span.start.min(src.len());
        let end = self.span.end.clamp(start, src.len());
        let range = start..end;

        let mut label = Label::new((file, range.clone())).with_color(Color::Red);
        if let Some(msg) = &self.label {
            label = label.with_message(msg);
        }
        // Spans are byte offsets; ariadne counts chars unless told otherwise,
        // which misplaces every label after a non-ASCII character.
        let mut report = Report::build(ReportKind::Error, (file, range))
            .with_config(Config::default().with_index_type(IndexType::Byte))
            .with_message(&self.title)
            .with_label(label);
        if let Some(help) = &self.help {
            report = report.with_help(help);
        }
        // Rendering only fails on I/O errors writing to stderr; nothing useful to do then.
        let _ = report.finish().eprint((file, Source::from(src)));
    }
}

/// Print every diagnostic, in source order.
pub fn eprint_all(diagnostics: &[Diagnostic], file: &str, src: &str) {
    for d in diagnostics {
        d.eprint(file, src);
    }
}
