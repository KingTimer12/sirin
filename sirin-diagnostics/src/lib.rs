pub mod span;

use ariadne::{Color, Label, Report, ReportKind, Source};

use crate::span::Span;

pub fn report_error(file: &str, src: &str, span: &Span, title: &str, message: &str) {
    let range = span.start..span.end;
    Report::build(ReportKind::Error, (file, range.clone()))
        .with_message(title)
        .with_label(
            Label::new((file, range))
                .with_message(message)
                .with_color(Color::Red),
        )
        .finish()
        .print((file, Source::from(src)))
        .unwrap();
}
