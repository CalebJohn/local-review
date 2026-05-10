use std::collections::HashMap;

mod canonical;

use crate::diff::types::{ChangeKind, DiffHunk};
use crate::syntax::mapping::MAX_HIGHLIGHT_BYTES;

/// Classify diff lines as formatting-only or semantic.
///
/// Parses `old_content` and `new_content` with the given tree-sitter language,
/// extracts tokens per line, and compares the canonical form of each change
/// group within each hunk. Lines in groups whose canonical forms match are
/// marked `formatting_only = true`.
///
/// Mutates `hunks` in place. If the language is `None` or either content
/// exceeds `MAX_HIGHLIGHT_BYTES`, classification is skipped (all lines
/// remain as-is with `formatting_only = false`).
pub fn classify_diff(
    hunks: &mut [DiffHunk],
    old_content: &str,
    new_content: &str,
    lang: Option<tree_sitter::Language>,
    ext: &str,
) {
    let Some(lang) = lang else { return };
    if old_content.len() > MAX_HIGHLIGHT_BYTES || new_content.len() > MAX_HIGHLIGHT_BYTES {
        return;
    }

    let old_tokens = extract_tokens(old_content, &lang);
    let new_tokens = extract_tokens(new_content, &lang);
    let (Some(old_tokens), Some(new_tokens)) = (old_tokens, new_tokens) else {
        return;
    };

    let qe = quotes_equivalent(ext);
    for hunk in hunks.iter_mut() {
        classify_hunk(hunk, &old_tokens, &new_tokens, qe);
    }
}

/// Identify change groups within a hunk and classify each group.
///
/// A change group is a maximal run of consecutive non-Equal lines.
/// Within a group, Delete lines belong to the old side and Insert lines
/// to the new side. If the canonical forms of both sides match, every
/// line in the group is marked `formatting_only = true`.
fn classify_hunk(
    hunk: &mut DiffHunk,
    old_tokens: &HashMap<u32, Vec<String>>,
    new_tokens: &HashMap<u32, Vec<String>>,
    quotes_equivalent: bool,
) {
    let lines = &mut hunk.lines;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind == ChangeKind::Equal {
            i += 1;
            continue;
        }

        // Start of a change group: collect consecutive non-Equal lines
        let group_start = i;
        while i < lines.len() && lines[i].kind != ChangeKind::Equal {
            i += 1;
        }
        let group_end = i;

        // Separate Delete and Insert lines by their line numbers
        let mut delete_lines: Vec<u32> = Vec::new();
        let mut insert_lines: Vec<u32> = Vec::new();
        for line in &lines[group_start..group_end] {
            if line.kind == ChangeKind::Delete {
                if let Some(ln) = line.old_lineno {
                    delete_lines.push(ln);
                }
            } else if line.kind == ChangeKind::Insert {
                if let Some(ln) = line.new_lineno {
                    insert_lines.push(ln);
                }
            }
        }

        // One-sided groups (pure Insert or pure Delete): formatting-only
        // if all lines are whitespace-only (no tokens on any line).
        if delete_lines.is_empty() || insert_lines.is_empty() {
            let token_map = if delete_lines.is_empty() { new_tokens } else { old_tokens };
            let line_nums = if delete_lines.is_empty() { &insert_lines } else { &delete_lines };
            let all_whitespace = line_nums.iter().all(|ln| {
                token_map.get(ln).is_none_or(|toks| toks.is_empty())
            });
            if all_whitespace {
                for line in &mut lines[group_start..group_end] {
                    line.formatting_only = true;
                }
            }
            continue;
        }

        let old_group_tokens = canonical::collect_tokens(old_tokens, &delete_lines);
        let new_group_tokens = canonical::collect_tokens(new_tokens, &insert_lines);

        if canonical::compare_canonical(&old_group_tokens, &new_group_tokens, quotes_equivalent) {
            for line in &mut lines[group_start..group_end] {
                line.formatting_only = true;
            }
        }
    }
}

/// Map a file extension to a tree-sitter Language for parsing.
/// Reuses the same extension-to-name mapping from `syntax::lang_for_extension`,
/// then resolves the name to a `tree_sitter::Language`.
pub fn language_for_extension(ext: &str) -> Option<tree_sitter::Language> {
    let lang_name = crate::syntax::lang_for_extension(ext)?;
    match lang_name {
        "rust"       => Some(tree_sitter_rust::LANGUAGE.into()),
        "typescript" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx"        => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "javascript" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "python"     => Some(tree_sitter_python::LANGUAGE.into()),
        "go"         => Some(tree_sitter_go::LANGUAGE.into()),
        "c"          => Some(tree_sitter_c::LANGUAGE.into()),
        "cpp"        => Some(tree_sitter_cpp::LANGUAGE.into()),
        "json"       => Some(tree_sitter_json::LANGUAGE.into()),
        "yaml"       => Some(tree_sitter_yaml::LANGUAGE.into()),
        "toml"       => Some(tree_sitter_toml_ng::LANGUAGE.into()),
        _            => None,
    }
}

fn quotes_equivalent(ext: &str) -> bool {
    matches!(ext, "py" | "js" | "jsx" | "ts" | "tsx")
}

/// Extract tokens (leaf-node text) from source code, keyed by 1-based line number.
///
/// Parses the source with tree-sitter, walks the AST, and collects the text of
/// every leaf node (nodes with no children) that is not purely whitespace.
///
/// Returns `None` if the source exceeds `MAX_HIGHLIGHT_BYTES` (256KB).
pub fn extract_tokens(source: &str, lang: &tree_sitter::Language) -> Option<HashMap<u32, Vec<String>>> {
    if source.len() > MAX_HIGHLIGHT_BYTES {
        return None;
    }

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(lang).ok()?;

    let tree = parser.parse(source, None)?;
    let root = tree.root_node();

    let mut result: HashMap<u32, Vec<String>> = HashMap::new();
    walk_node(&root, source.as_bytes(), &mut result);

    Some(result)
}

fn walk_node(
    node: &tree_sitter::Node,
    source: &[u8],
    result: &mut HashMap<u32, Vec<String>>,
) {
    if node.child_count() == 0 {
        // Leaf node: extract text
        let text = node.utf8_text(source).unwrap_or("");
        if !text.trim().is_empty() {
            let line = node.start_position().row as u32 + 1; // 1-based
            result.entry(line).or_default().push(text.to_string());
        }
    } else {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk_node(&child, source, result);
        }
    }
}

#[cfg(test)]
mod tests;
