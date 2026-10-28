//! Text helpers shared by the connectors.

use crate::fact::Line;

/// Maps byte offsets to 1-based line numbers, for parsers that report byte spans.
#[derive(Debug, Clone)]
pub struct LineIndex {
    starts: Vec<u32>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = Vec::with_capacity(text.len() / 32 + 1);
        starts.push(0);
        starts.extend(
            memchr::memchr_iter(b'\n', text.as_bytes())
                .map(|newline| u32::try_from(newline + 1).unwrap_or(u32::MAX)),
        );
        Self { starts }
    }

    /// The line holding the byte at `offset`.
    pub fn line(&self, offset: u32) -> Line {
        let line = self.starts.partition_point(|&start| start <= offset);
        u32::try_from(line).unwrap_or(u32::MAX).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_map_to_lines() {
        let index = LineIndex::new("ab\ncd\n\nef");
        let lines: Vec<Line> = [0, 2, 3, 6, 7, 99]
            .map(|offset| index.line(offset))
            .to_vec();
        assert_eq!(lines, vec![1, 1, 2, 3, 4, 4]);
    }
}
