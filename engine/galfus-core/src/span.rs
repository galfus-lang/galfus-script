#[cfg(test)]
mod tests;

use crate::SourceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    source_id: SourceId,
    start: usize,
    end: usize,
}

impl Span {
    pub const fn new(source_id: SourceId, start: usize, end: usize) -> Self {
        Self {
            source_id,
            start,
            end,
        }
    }

    pub const fn empty(source_id: SourceId, offset: usize) -> Self {
        Self {
            source_id,
            start: offset,
            end: offset,
        }
    }

    pub const fn source_id(self) -> SourceId {
        self.source_id
    }

    pub const fn start(self) -> usize {
        self.start
    }

    pub const fn end(self) -> usize {
        self.end
    }

    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub const fn contains(self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }

    pub fn checked_new(source_id: SourceId, start: usize, end: usize) -> Option<Self> {
        if start <= end {
            Some(Self::new(source_id, start, end))
        } else {
            None
        }
    }

    pub fn cover(self, other: Self) -> Option<Self> {
        if self.source_id != other.source_id {
            return None;
        }

        Some(Self {
            source_id: self.source_id,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        })
    }
}
