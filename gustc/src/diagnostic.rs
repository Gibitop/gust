use crate::span::{SourceMap, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub span: Span,
    pub message: String,
}

impl Diagnostic {
    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            span,
            message: message.into(),
        }
    }

    pub fn warning(span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            span,
            message: message.into(),
        }
    }

    pub fn render(&self, path: &str, source_map: &SourceMap<'_>) -> String {
        let location = source_map.location(self.span.start);
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };

        format!(
            "{path}:{}:{}: {severity}: {}",
            location.line, location.column, self.message
        )
    }
}
