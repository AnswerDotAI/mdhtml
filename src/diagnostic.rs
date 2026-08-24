use crate::line::SourceSpan;
use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Severity {
    #[default]
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub span: Option<SourceSpan>,
}

impl Diagnostic {
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { severity: Severity::Warning, code: code.into(), message: message.into(), span: None }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { severity: Severity::Error, code: code.into(), message: message.into(), span: None }
    }

    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(loc) = self.span.and_then(|s| s.start_location) {
            write!(f, "line {}: ", loc.line)?;
        }
        f.write_str(&self.message)
    }
}
