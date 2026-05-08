use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{App, AppMode, Focus, SidebarSection};

pub fn footer_line<'a>(app: &'a App) -> Line<'a> {
    let key_style = Style::default().fg(Color::White).bg(Color::DarkGray).add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);
    let sep = Span::styled("  ", desc_style);

    if let Some(ref msg) = app.status_message {
        return Line::from(vec![Span::styled(msg.clone(), Style::default().fg(Color::Red))]);
    }

    let in_staged = app.sidebar_section == SidebarSection::Staged;

    let mut spans: Vec<Span> = Vec::new();
    match app.focus {
        Focus::Sidebar => {
            if in_staged {
                spans.extend([Span::styled(" u ", key_style), Span::styled("unstage", desc_style), sep.clone()]);
            } else {
                spans.extend([Span::styled(" s ", key_style), Span::styled("stage", desc_style), sep.clone()]);
                spans.extend([Span::styled(" d ", key_style), Span::styled("discard", desc_style), sep.clone()]);
            }
            spans.extend([Span::styled(" Enter ", key_style), Span::styled("diff", desc_style), sep.clone()]);
            spans.extend([Span::styled(" e ", key_style), Span::styled("edit", desc_style)]);
        }
        Focus::DiffView if app.diff_stale => {
            let warn_style = Style::default().fg(Color::Yellow);
            spans.extend([Span::styled("file changed", warn_style), sep.clone()]);
            spans.extend([Span::styled(" r ", key_style), Span::styled("reload", desc_style)]);
        }
        Focus::DiffView => {
            if app.mode == AppMode::Visual {
                spans.extend([Span::styled("[VISUAL]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)), sep.clone()]);
                if in_staged {
                    spans.extend([Span::styled(" u ", key_style), Span::styled("unstage", desc_style), sep.clone()]);
                } else {
                    spans.extend([Span::styled(" s ", key_style), Span::styled("stage", desc_style), sep.clone()]);
                }
                spans.extend([Span::styled(" c ", key_style), Span::styled("comment", desc_style), sep.clone()]);
                spans.extend([Span::styled(" Esc ", key_style), Span::styled("exit", desc_style)]);
            } else {
                spans.extend([Span::styled(" n/N ", key_style), Span::styled("hunks", desc_style), sep.clone()]);
                if in_staged {
                    spans.extend([Span::styled(" u/U ", key_style), Span::styled("unstage", desc_style), sep.clone()]);
                } else {
                    spans.extend([Span::styled(" s/S ", key_style), Span::styled("stage", desc_style), sep.clone()]);
                    spans.extend([Span::styled(" d/D ", key_style), Span::styled("discard", desc_style), sep.clone()]);
                }
                spans.extend([Span::styled(" v ", key_style), Span::styled("visual", desc_style), sep.clone()]);
                spans.extend([Span::styled(" c ", key_style), Span::styled("comment", desc_style), sep.clone()]);
                spans.extend([Span::styled(" w ", key_style), Span::styled(
                    if app.semantic_filter { "show all" } else { "filter ws" },
                    desc_style,
                )]);
                if app.semantic_filter {
                    if let Some((visible, total, hidden)) = app.hunk_counts() {
                        spans.extend([sep.clone(), Span::styled(
                            format!("{}/{} ({} hidden)", visible, total, hidden),
                            Style::default().fg(Color::Cyan),
                        )]);
                    }
                }
            }
        }
        Focus::CommentInput => {
            spans.extend([
                Span::styled("comment: ", Style::default().fg(Color::White)),
                Span::raw(&app.comment_input),
                Span::raw("\u{2588}"),
            ]);
        }
    }

    Line::from(spans)
}

pub fn render_footer(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    frame.render_widget(Paragraph::new(footer_line(app)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::HashMap;
    use crate::app::{App, AppMode, Focus, SidebarSection};
    use crate::git::GitRepo;
    use crate::git::types::{FileEntry, FileStatus};
    use crate::undo::UndoManager;

    fn test_app() -> App {
        let repo = GitRepo::open(".").expect("repo should open");
        App {
            repo,
            staged_files: vec![FileEntry {
                path: "staged.rs".to_string(),
                index_status: Some(FileStatus::Modified),
                workdir_status: None,
            }],
            unstaged_files: vec![FileEntry {
                path: "unstaged.rs".to_string(),
                index_status: None,
                workdir_status: Some(FileStatus::Modified),
            }],
            selected_index: 0,
            sidebar_section: SidebarSection::Unstaged,
            diff_content: None,
            diff_scroll: 0,
            focus: Focus::Sidebar,
            should_quit: false,
            styled_diff: None,
            current_hunk_index: None,
            scroll_positions: HashMap::new(),
            diff_stale: false,
            auto_reload: false,
            status_message: None,
            sidebar_collapsed: false,
            pending_discard: None,
            show_full_file: false,
            diff_viewport_height: Cell::new(0),
            undo: UndoManager::new(),
            comment_input: String::new(),
            comment_context: None,
            mode: AppMode::Normal,
            diff_cursor: 0,
            visual_selection: Vec::new(),
            visual_cursor: 0,
            visual_anchor: 0,
            visual_from_mouse: false,
            semantic_filter: false,
            formatting_only_cache: HashMap::new(),
        }
    }

    fn span_text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect::<String>()
    }

    #[test]
    fn sidebar_unstaged_shows_stage_discard_enter_edit() {
        let app = test_app();
        let line = footer_line(&app);
        let text = span_text(&line);
        assert!(text.contains("stage"), "should show stage");
        assert!(text.contains("discard"), "should show discard");
        assert!(text.contains("Enter"), "should show Enter");
        assert!(text.contains("edit"), "should show edit");
    }

    #[test]
    fn sidebar_unstaged_omits_discoverable_keys() {
        let app = test_app();
        let line = footer_line(&app);
        let text = span_text(&line);
        assert!(!text.contains("quit"), "q is discoverable");
        assert!(!text.contains("navigate"), "j/k is discoverable");
        assert!(!text.contains("switch pane"), "Tab is discoverable");
        assert!(!text.contains("sidebar"), "b is discoverable");
    }

    #[test]
    fn sidebar_staged_shows_unstage_not_stage() {
        let mut app = test_app();
        app.sidebar_section = SidebarSection::Staged;
        let line = footer_line(&app);
        let text = span_text(&line);
        assert!(text.contains("unstage"), "should show unstage");
        assert!(!text.contains("discard"), "no discard in staged");
    }

    #[test]
    fn diffview_unstaged_shows_compact_keys() {
        let mut app = test_app();
        app.focus = Focus::DiffView;
        let line = footer_line(&app);
        let text = span_text(&line);
        assert!(text.contains("n/N"), "combined hunk nav");
        assert!(text.contains("s/S"), "combined stage keys");
        assert!(text.contains("d/D"), "combined discard keys");
        assert!(text.contains("visual"), "v visual");
        assert!(text.contains("comment"), "c comment");
    }

    #[test]
    fn diffview_unstaged_omits_discoverable_keys() {
        let mut app = test_app();
        app.focus = Focus::DiffView;
        let line = footer_line(&app);
        let text = span_text(&line);
        assert!(!text.contains("quit"), "q is discoverable");
        assert!(!text.contains("navigate"), "j/k is discoverable");
        assert!(!text.contains("switch pane"), "Tab is discoverable");
    }

    #[test]
    fn diffview_staged_shows_unstage() {
        let mut app = test_app();
        app.focus = Focus::DiffView;
        app.sidebar_section = SidebarSection::Staged;
        let line = footer_line(&app);
        let text = span_text(&line);
        assert!(text.contains("u/U"), "combined unstage keys");
        assert!(text.contains("unstage"));
        assert!(!text.contains("discard"), "no discard in staged");
    }

    #[test]
    fn diffview_stale_shows_reload_only() {
        let mut app = test_app();
        app.focus = Focus::DiffView;
        app.diff_stale = true;
        let line = footer_line(&app);
        let text = span_text(&line);
        assert!(text.contains("file changed"), "stale warning");
        assert!(text.contains("reload"), "reload hint");
        assert!(!text.contains("stage"), "no stage in stale");
    }

    #[test]
    fn diffview_visual_shows_visual_indicator() {
        let mut app = test_app();
        app.focus = Focus::DiffView;
        app.mode = AppMode::Visual;
        let line = footer_line(&app);
        let text = span_text(&line);
        assert!(text.contains("[VISUAL]"), "visual indicator");
        assert!(text.contains("stage"), "s stage in visual");
        assert!(text.contains("comment"), "c comment in visual");
        assert!(text.contains("Esc"), "Esc exit in visual");
    }

    #[test]
    fn diffview_visual_staged_shows_unstage() {
        let mut app = test_app();
        app.focus = Focus::DiffView;
        app.mode = AppMode::Visual;
        app.sidebar_section = SidebarSection::Staged;
        let line = footer_line(&app);
        let text = span_text(&line);
        assert!(text.contains("unstage"));
        assert!(!text.contains(" s "), "no stage key in staged visual");
    }

    #[test]
    fn status_message_overrides_footer() {
        let mut app = test_app();
        app.status_message = Some("error occurred".to_string());
        let line = footer_line(&app);
        let text = span_text(&line);
        assert_eq!(text, "error occurred");
    }

    #[test]
    fn comment_input_shows_input_field() {
        let mut app = test_app();
        app.focus = Focus::CommentInput;
        app.comment_input = "hello".to_string();
        let line = footer_line(&app);
        let text = span_text(&line);
        assert!(text.contains("comment:"));
        assert!(text.contains("hello"));
    }

    #[test]
    fn diffview_semantic_filter_shows_counts() {
        let mut app = test_app();
        app.focus = Focus::DiffView;
        app.semantic_filter = true;
        let line = footer_line(&app);
        let text = span_text(&line);
        assert!(text.contains("show all"), "filter toggle label");
    }
}
