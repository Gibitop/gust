#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn join(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
}

pub struct SourceMap<'source> {
    source: &'source str,
    line_starts: Vec<usize>,
}

impl<'source> SourceMap<'source> {
    pub fn new(source: &'source str) -> Self {
        let mut line_starts = vec![0];

        for (index, character) in source.char_indices() {
            if character == '\n' {
                line_starts.push(index + character.len_utf8());
            }
        }

        Self {
            source,
            line_starts,
        }
    }

    pub fn location(&self, offset: usize) -> SourceLocation {
        let clamped_offset = offset.min(self.source.len());
        let line_index = match self.line_starts.binary_search(&clamped_offset) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        };
        let line_start = self.line_starts[line_index];

        SourceLocation {
            line: line_index + 1,
            column: clamped_offset.saturating_sub(line_start) + 1,
        }
    }
}
