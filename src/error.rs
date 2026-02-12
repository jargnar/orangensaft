use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn new(start: usize, end: usize, line: usize, col: usize) -> Self {
        Self {
            start,
            end,
            line,
            col,
        }
    }

    pub fn merge(left: Span, right: Span) -> Self {
        let (start, line, col) = if left.start <= right.start {
            (left.start, left.line, left.col)
        } else {
            (right.start, right.line, right.col)
        };

        let end = left.end.max(right.end);
        Self {
            start,
            end,
            line,
            col,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SaftError {
    pub message: String,
    pub span: Option<Span>,
}

impl SaftError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: None,
        }
    }

    pub fn with_span(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span: Some(span),
        }
    }

    pub fn render(&self, file_path: &str, source: &str) -> String {
        match self.span {
            Some(span) => {
                let line_text = source
                    .lines()
                    .nth(span.line.saturating_sub(1))
                    .unwrap_or_default();

                let caret_pad = " ".repeat(span.col.saturating_sub(1));
                let width = span.end.saturating_sub(span.start).max(1);
                let carets = "^".repeat(width.min(120));

                format!(
                    "error: {}\n  --> {}:{}:{}\n   |\n{:>3} | {}\n   | {}{}",
                    self.message,
                    file_path,
                    span.line,
                    span.col,
                    span.line,
                    line_text,
                    caret_pad,
                    carets
                )
            }
            None => format!("error: {} ({file_path})", self.message),
        }
    }
}

impl fmt::Display for SaftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.span {
            Some(span) => write!(f, "{} at {}:{}", self.message, span.line, span.col),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for SaftError {}

pub type SaftResult<T> = Result<T, SaftError>;
