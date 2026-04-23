//! Byte-offset -> per-line styled span mapping.
//!
//! Plan 02-02 implements the line_starts + partition_point + emit_spans algorithm.

use tree_sitter_highlight::{HighlightEvent, Highlighter};

use ratatui::style::Style;

use crate::syntax::registry::registry;
use crate::syntax::scope::scope_to_style;
use crate::syntax::{StyledLine, StyledSpan, lang_for_extension};

pub const MAX_HIGHLIGHT_BYTES: usize = 256 * 1024;

pub fn highlight_source_inner(source: &str, extension: Option<&str>) -> Option<Vec<StyledLine>> {
    if source.len() > MAX_HIGHLIGHT_BYTES { return None; }

    let lang = lang_for_extension(extension?)?;
    let reg = registry();
    let cfg = reg.get(lang)?;

    let line_starts = compute_line_starts(source);
    let num_lines = line_starts.len().max(1);

    let mut result: Vec<StyledLine> = (0..num_lines).map(|_| Vec::new()).collect();

    let mut highlighter = Highlighter::new();
    let iter = highlighter
        .highlight(cfg, source.as_bytes(), None, |_| None)
        .ok()?;

    let mut stack: Vec<tree_sitter_highlight::Highlight> = Vec::new();

    for event in iter {
        match event.ok()? {
            HighlightEvent::HighlightStart(h) => stack.push(h),
            HighlightEvent::HighlightEnd      => { stack.pop(); }
            HighlightEvent::Source { start, end } => {
                let style = stack
                    .last()
                    .map(|h| scope_to_style(*h))
                    .unwrap_or_default();
                emit_spans(source, start, end, style, &line_starts, &mut result);
            }
        }
    }

    Some(result)
}

pub fn compute_line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

pub fn emit_spans(
    source: &str,
    range_start: usize,
    range_end: usize,
    style: Style,
    line_starts: &[usize],
    result: &mut [StyledLine],
) {
    if range_end <= range_start { return; }

    let mut line = line_starts.partition_point(|&s| s <= range_start).saturating_sub(1);

    let mut byte = range_start;
    while byte < range_end {
        let line_end = line_starts.get(line + 1).copied().unwrap_or(source.len());
        let chunk_end = range_end.min(line_end);

        let mut slice_end = chunk_end;
        if slice_end > byte && source.as_bytes().get(slice_end - 1) == Some(&b'\n') {
            slice_end -= 1;
        }

        if slice_end > byte {
            if let Some(text) = source.get(byte..slice_end) {
                if let Some(line_vec) = result.get_mut(line) {
                    line_vec.push(StyledSpan {
                        text: text.to_string(),
                        style,
                    });
                }
            }
        }

        byte = chunk_end;
        line += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_line_starts_empty() {
        assert_eq!(compute_line_starts(""), vec![0]);
    }

    #[test]
    fn test_compute_line_starts_no_newline() {
        assert_eq!(compute_line_starts("abc"), vec![0]);
    }

    #[test]
    fn test_compute_line_starts_two_lines() {
        assert_eq!(compute_line_starts("abc\ndef"), vec![0, 4]);
    }

    #[test]
    fn test_compute_line_starts_trailing_newline() {
        assert_eq!(compute_line_starts("abc\ndef\n"), vec![0, 4, 8]);
    }

    #[test]
    fn test_highlight_source_inner_unknown_extension() {
        assert!(highlight_source_inner("fn main() {}", Some("xyz")).is_none());
        assert!(highlight_source_inner("fn main() {}", None).is_none());
    }

    #[test]
    fn test_highlight_source_inner_oversize() {
        let big = "x".repeat(MAX_HIGHLIGHT_BYTES + 1);
        assert!(highlight_source_inner(&big, Some("rs")).is_none());
    }

    #[test]
    fn test_highlight_source_inner_rust_keyword_colored() {
        let lines = highlight_source_inner("fn main() {}", Some("rs"))
            .expect("rust highlight must succeed on valid input");
        assert_eq!(lines.len(), 1, "one-line input -> one StyledLine");
        let has_colored = lines[0].iter().any(|s| s.style != Style::default());
        assert!(has_colored, "expected at least one non-default-style span for `fn main() {{}}`, got {:?}", lines[0]);
    }

    #[test]
    fn test_highlight_source_inner_multiline() {
        let src = "fn a() {}\nfn b() {}\n";
        let lines = highlight_source_inner(src, Some("rs"))
            .expect("rust highlight must succeed");
        assert!(lines.len() >= 2, "expected >=2 lines, got {}", lines.len());
    }

    #[test]
    fn test_highlight_source_inner_utf8_safe() {
        let src = "// 🦀 crab comment\nfn x() {}\n";
        let result = highlight_source_inner(src, Some("rs"));
        assert!(result.is_some(), "UTF-8 input should produce Some(_), not panic or None");
    }

    #[test]
    fn test_emit_spans_trims_trailing_newline() {
        let src = "abc\n";
        let line_starts = compute_line_starts(src);
        let mut out: Vec<StyledLine> = vec![Vec::new(), Vec::new()];
        emit_spans(src, 0, 4, Style::default(), &line_starts, &mut out);
        assert_eq!(out[0].len(), 1);
        assert_eq!(out[0][0].text, "abc");
    }

    #[test]
    fn test_emit_spans_spans_two_lines() {
        let src = "ab\ncd";
        let line_starts = compute_line_starts(src);
        let mut out: Vec<StyledLine> = vec![Vec::new(), Vec::new()];
        emit_spans(src, 0, src.len(), Style::default(), &line_starts, &mut out);
        assert_eq!(out[0][0].text, "ab");
        assert_eq!(out[1][0].text, "cd");
    }
}