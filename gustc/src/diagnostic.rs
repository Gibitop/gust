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
            message: format_diagnostic_message(&message.into()),
        }
    }

    pub fn warning(span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            span,
            message: format_diagnostic_message(&message.into()),
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

/// Formats compiler names for diagnostics using Gust source syntax.
///
/// Imported declarations carry deterministic linker-qualified names internally
/// so that unrelated modules can declare the same source name. Those qualifiers
/// are not part of Gust source syntax and must never reach a user diagnostic.
pub fn format_diagnostic_name(name: &str) -> String {
    let mut formatted = String::with_capacity(name.len());
    let mut index = 0;

    while index < name.len() {
        let rest = &name[index..];
        let prefix_len = if rest.starts_with("module_extension_") {
            "module_extension_".len()
        } else if rest.starts_with("module_") {
            "module_".len()
        } else {
            0
        };

        if prefix_len > 0 {
            let hash_start = index + prefix_len;
            let hash_end = hash_start + 8;
            if name
                .get(hash_start..hash_end)
                .is_some_and(|hash| hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
                && name.get(hash_end..hash_end + 2) == Some("::")
            {
                index = hash_end + 2;
                continue;
            }
        }

        let character = rest.chars().next().expect("index stays within the string");
        formatted.push(character);
        index += character.len_utf8();
    }

    formatted.replace("::", ".")
}

fn format_diagnostic_message(message: &str) -> String {
    format_diagnostic_name(message)
}

#[cfg(test)]
mod tests {
    use super::format_diagnostic_name;

    #[test]
    fn formats_linker_qualified_generic_names_as_gust_source() {
        assert_eq!(
            format_diagnostic_name("module_4f7a8c2d::ArrayList<module_extension_12ab34cd::Item>",),
            "ArrayList<Item>",
        );
    }

    #[test]
    fn uses_dots_for_remaining_qualified_names() {
        assert_eq!(format_diagnostic_name("Trait::method"), "Trait.method");
    }
}
