#[cfg(test)]
mod tests;

use crate::{SourceId, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RowCol {
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceFile {
    id: SourceId,
    name: String,
    text: String,
    row_starts: Vec<usize>,
}

impl SourceFile {
    pub fn new(id: SourceId, name: String, text: String) -> Self {
        let mut row_starts = Vec::new();

        row_starts.push(0);

        for (byte_index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                row_starts.push(byte_index + 1);
            }
        }

        Self {
            id,
            name,
            text,
            row_starts,
        }
    }

    pub fn id(&self) -> SourceId {
        self.id
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn text(&self) -> &str {
        self.text.as_str()
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.len() == 0
    }

    pub fn row_col(&self, offset: usize) -> Option<RowCol> {
        let len = self.len();

        if offset > len {
            return None;
        }

        let index = match self.row_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(0) => return None,
            Err(i) => i - 1,
        };
        let start = self.row_starts[index];

        Some(RowCol {
            row: index + 1,
            column: offset - start + 1,
        })
    }

    pub fn offset(&self, line_col: &RowCol) -> Option<usize> {
        let row_index = line_col.row.checked_sub(1)?;
        let column_offset = line_col.column.checked_sub(1)?;
        let row_start = *self.row_starts.get(row_index)?;
        Some(row_start + column_offset)
    }

    pub fn slice(&self, span: Span) -> Option<&str> {
        if span.source_id() != self.id {
            return None;
        }

        self.text.get(span.start()..span.end())
    }

    pub fn span(&self) -> Span {
        Span::new(self.id, 0, self.len())
    }
}
