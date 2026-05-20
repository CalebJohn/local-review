use ratatui::prelude::*;
use tree_sitter_highlight::Highlight;

use crate::syntax::registry::HIGHLIGHT_NAMES;

pub fn scope_to_style(h: Highlight) -> Style {
    match HIGHLIGHT_NAMES.get(h.0).copied().unwrap_or("") {
        "comment"                                         => Style::default().fg(Color::DarkGray),
        "string" | "string.special"                       => Style::default().fg(Color::Green),
        "number" | "constant" | "constant.builtin"        => Style::default().fg(Color::Magenta),
        "keyword" | "tag"                                 => Style::default().fg(Color::Blue),
        "function" | "function.builtin" | "function.method" => Style::default().fg(Color::Cyan),
        "type" | "type.builtin" | "constructor"           => Style::default().fg(Color::Yellow),
        "attribute" | "property"                          => Style::default().fg(Color::Yellow),
        _ => Style::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_to_style_out_of_bounds_is_default() {
        let s = scope_to_style(Highlight(9999));
        assert_eq!(s, Style::default());
    }
}