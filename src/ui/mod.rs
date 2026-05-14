mod diff_view;
mod footer;
mod sidebar;

use ratatui::prelude::*;
use ratatui::layout::Constraint;

use crate::app::App;

#[cfg(test)]
pub use diff_view::diff_lines;

pub fn case_insensitive_match_ranges(text: &str, pattern: &str) -> Vec<(usize, usize)> {
    let pat_chars: Vec<char> = pattern.chars().flat_map(|c| c.to_lowercase()).collect();
    if pat_chars.is_empty() {
        return vec![];
    }
    let text_chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut ranges = Vec::new();
    'outer: for i in 0..text_chars.len() {
        let mut pi = 0;
        let mut ti = i;
        while pi < pat_chars.len() && ti < text_chars.len() {
            for lc in text_chars[ti].1.to_lowercase() {
                if pi >= pat_chars.len() || lc != pat_chars[pi] {
                    continue 'outer;
                }
                pi += 1;
            }
            ti += 1;
        }
        if pi == pat_chars.len() {
            let byte_start = text_chars[i].0;
            let byte_end = if ti < text_chars.len() { text_chars[ti].0 } else { text.len() };
            ranges.push((byte_start, byte_end));
        }
    }
    ranges
}

pub fn contains_match(text: &str, pattern: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        text.contains(pattern)
    } else {
        !case_insensitive_match_ranges(text, pattern).is_empty()
    }
}

pub fn view(frame: &mut ratatui::Frame, app: &App) {
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());
    if app.sidebar_collapsed {
        diff_view::render_diff_view(frame, app, rows[0]);
    } else {
        let cols = Layout::horizontal([Constraint::Length(30), Constraint::Min(1)])
            .split(rows[0]);
        sidebar::render_sidebar(frame, app, cols[0]);
        diff_view::render_diff_view(frame, app, cols[1]);
    }
    footer::render_footer(frame, app, rows[1]);
}

pub fn sidebar_section_areas(sidebar_area: Rect, staged_count: usize, unstaged_count: usize) -> (Rect, Rect) {
    let total = staged_count + unstaged_count;
    if total == 0 {
        let halves = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(sidebar_area);
        return (halves[0], halves[1]);
    }

    let min_rows: u16 = 3;
    let available = sidebar_area.height;

    if available < min_rows * 2 {
        let halves = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(sidebar_area);
        return (halves[0], halves[1]);
    }

    let staged_content = staged_count as u16 + 2;
    let unstaged_content = unstaged_count as u16 + 2;
    let total_wanted = staged_content + unstaged_content;

    let (staged_h, unstaged_h) = if total_wanted <= available {
        (staged_content, available - staged_content)
    } else {
        let staged_ratio = staged_count as f32 / total as f32;
        let staged_h = ((available as f32 * staged_ratio) as u16).max(min_rows);
        let unstaged_h = available.saturating_sub(staged_h).max(min_rows);
        let staged_h = available.saturating_sub(unstaged_h);
        (staged_h, unstaged_h)
    };

    let sections = Layout::vertical([Constraint::Length(staged_h), Constraint::Length(unstaged_h)])
        .split(sidebar_area);
    (sections[0], sections[1])
}

fn border_color(focused: bool) -> Color {
    if focused {
        Color::Blue
    } else {
        Color::DarkGray
    }
}

#[cfg(test)]
#[path = "../ui_tests.rs"]
mod tests;

#[cfg(test)]
mod match_range_tests {
    use super::{case_insensitive_match_ranges, contains_match};

    #[test]
    fn ascii_case_insensitive() {
        assert_eq!(case_insensitive_match_ranges("Hello World", "hello"), vec![(0, 5)]);
    }

    #[test]
    fn multiple_matches() {
        assert_eq!(case_insensitive_match_ranges("foo bar foo", "foo"), vec![(0, 3), (8, 11)]);
    }

    #[test]
    fn no_match() {
        assert!(case_insensitive_match_ranges("hello", "xyz").is_empty());
    }

    #[test]
    fn empty_pattern() {
        assert!(case_insensitive_match_ranges("hello", "").is_empty());
    }

    #[test]
    fn multibyte_text_no_panic() {
        let result = case_insensitive_match_ranges("café résumé", "e");
        for (s, e) in &result {
            let _ = &"café résumé"[*s..*e];
        }
    }

    #[test]
    fn turkish_i_no_panic() {
        let result = case_insensitive_match_ranges("İstanbul", "i");
        for (s, e) in &result {
            let _ = &"İstanbul"[*s..*e];
        }
    }

    // ---- contains_match tests ----

    #[test]
    fn contains_match_case_sensitive_found() {
        assert!(contains_match("Hello World", "Hello", true));
    }

    #[test]
    fn contains_match_case_sensitive_not_found() {
        assert!(!contains_match("Hello World", "hello", true));
    }

    #[test]
    fn contains_match_case_insensitive_found() {
        assert!(contains_match("Hello World", "hello", false));
        assert!(contains_match("HELLO WORLD", "hello", false));
    }

    #[test]
    fn contains_match_case_insensitive_not_found() {
        assert!(!contains_match("Hello World", "xyz", false));
    }

    #[test]
    fn contains_match_empty_pattern_case_sensitive() {
        assert!(contains_match("hello", "", true));
    }

    #[test]
    fn contains_match_empty_pattern_case_insensitive() {
        assert!(!contains_match("hello", "", false));
    }

    #[test]
    fn contains_match_multibyte_text() {
        assert!(contains_match("café résumé", "café", false));
        assert!(contains_match("HELLO café", "hello", false));
    }

    #[test]
    fn contains_match_consistent_with_ranges() {
        let text = "Hello World café";
        let pattern = "hello";
        let ci_result = contains_match(text, pattern, false);
        let ranges_result = !case_insensitive_match_ranges(text, pattern).is_empty();
        assert_eq!(ci_result, ranges_result, "contains_match must agree with match_ranges");
    }
}
