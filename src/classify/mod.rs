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
        classify_hunk(hunk, &old_tokens, &new_tokens, qe, ext);
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
    ext: &str,
) {
    if classify_hunk_as_import_reorder(hunk, old_tokens, new_tokens, ext, quotes_equivalent) {
        return;
    }

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

        let is_formatting = canonical::compare_canonical(
            &old_group_tokens,
            &new_group_tokens,
            quotes_equivalent,
        ) || is_import_reorder(
            &delete_lines,
            &insert_lines,
            old_tokens,
            new_tokens,
            ext,
            quotes_equivalent,
        );

        if is_formatting {
            for line in &mut lines[group_start..group_end] {
                line.formatting_only = true;
            }
        }
    }
}

/// Check if an entire hunk represents an import reorder.
///
/// Collects all Delete and Insert lines across the hunk (which may be separated
/// by Equal lines due to the diff algorithm), checks they are all import-related,
/// and compares the sorted multisets of import statements.
///
/// Returns true and marks all non-Equal lines as formatting_only if the hunk is
/// purely an import reorder.
fn classify_hunk_as_import_reorder(
    hunk: &mut DiffHunk,
    old_tokens: &HashMap<u32, Vec<String>>,
    new_tokens: &HashMap<u32, Vec<String>>,
    ext: &str,
    quotes_equivalent: bool,
) -> bool {
    let mut delete_lines: Vec<u32> = Vec::new();
    let mut insert_lines: Vec<u32> = Vec::new();

    for line in hunk.lines.iter() {
        match line.kind {
            ChangeKind::Delete => {
                if let Some(ln) = line.old_lineno {
                    delete_lines.push(ln);
                }
            }
            ChangeKind::Insert => {
                if let Some(ln) = line.new_lineno {
                    insert_lines.push(ln);
                }
            }
            ChangeKind::Equal => {}
        }
    }

    if delete_lines.is_empty() || insert_lines.is_empty() {
        return false;
    }

    if !is_import_reorder(
        &delete_lines,
        &insert_lines,
        old_tokens,
        new_tokens,
        ext,
        quotes_equivalent,
    ) {
        return false;
    }

    for line in hunk.lines.iter_mut() {
        if line.kind != ChangeKind::Equal {
            line.formatting_only = true;
        }
    }
    true
}

/// Map a file extension to a tree-sitter Language for parsing.
/// Reuses the same extension-to-name mapping from `syntax::lang_for_extension`,
/// then resolves the name to a `tree_sitter::Language`.
pub fn language_for_extension(ext: &str) -> Option<tree_sitter::Language> {
    let lang_name = crate::syntax::lang_for_extension(ext)?;
    crate::syntax::language_for_name(lang_name)
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

/// Check if a change group is just reordered import statements.
///
/// Groups lines into logical import statements (handling multi-line imports),
/// normalizes each import's tokens, sorts both sides, and compares.
fn is_import_reorder(
    delete_lines: &[u32],
    insert_lines: &[u32],
    old_tokens: &HashMap<u32, Vec<String>>,
    new_tokens: &HashMap<u32, Vec<String>>,
    ext: &str,
    quotes_equivalent: bool,
) -> bool {
    let old_imports = group_imports(delete_lines, old_tokens, ext);
    let new_imports = group_imports(insert_lines, new_tokens, ext);

    let (Some(old_imports), Some(new_imports)) = (old_imports, new_imports) else {
        return false;
    };

    if old_imports.is_empty() && new_imports.is_empty() {
        return false;
    }

    let mut old_normalized: Vec<String> = old_imports
        .iter()
        .map(|tokens| {
            canonical::canonical_string(&canonical::normalize_tokens(
                &canonical::strip_trailing_commas(tokens),
                quotes_equivalent,
            ))
        })
        .collect();
    let mut new_normalized: Vec<String> = new_imports
        .iter()
        .map(|tokens| {
            canonical::canonical_string(&canonical::normalize_tokens(
                &canonical::strip_trailing_commas(tokens),
                quotes_equivalent,
            ))
        })
        .collect();

    old_normalized.sort();
    new_normalized.sort();

    old_normalized == new_normalized
}

/// Group lines into logical import statements, returning tokens per import.
///
/// Returns `None` if any non-blank line is not part of an import statement.
/// Tracks open delimiters to handle multi-line imports (e.g. `use foo::{a, b};`
/// split across lines).
fn group_imports(
    line_nums: &[u32],
    tokens: &HashMap<u32, Vec<String>>,
    ext: &str,
) -> Option<Vec<Vec<String>>> {
    let mut imports: Vec<Vec<String>> = Vec::new();
    let mut current_import: Vec<String> = Vec::new();
    let mut open_delimiters: i32 = 0;

    for &ln in line_nums {
        let line_tokens = tokens.get(&ln);
        let is_blank = line_tokens.is_none_or(|t| t.is_empty());

        if is_blank {
            if open_delimiters > 0 {
                continue;
            }
            if !current_import.is_empty() {
                imports.push(std::mem::take(&mut current_import));
            }
            continue;
        }

        let line_tokens = line_tokens.unwrap();

        if open_delimiters == 0 {
            if !current_import.is_empty() {
                imports.push(std::mem::take(&mut current_import));
            }

            if is_import_start(line_tokens, ext) {
                current_import.extend(line_tokens.iter().cloned());
            } else {
                return None;
            }
        } else {
            current_import.extend(line_tokens.iter().cloned());
        }

        for token in line_tokens {
            match token.as_str() {
                "(" | "{" | "[" => open_delimiters += 1,
                ")" | "}" | "]" => open_delimiters -= 1,
                _ => {}
            }
        }
    }

    if !current_import.is_empty() {
        imports.push(current_import);
    }

    if open_delimiters != 0 {
        return None;
    }

    Some(imports)
}

fn is_import_start(tokens: &[String], ext: &str) -> bool {
    if tokens.is_empty() {
        return false;
    }
    match ext {
        "rs" => tokens[0] == "use",
        "py" | "pyi" => tokens[0] == "import" || tokens[0] == "from",
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" => tokens[0] == "import",
        "go" => {
            tokens[0] == "import"
                || tokens[0].starts_with('"')
                || (tokens.len() >= 2 && tokens[1].starts_with('"'))
        }
        "c" | "h" => {
            tokens[0] == "#include"
                || (tokens.len() >= 2 && tokens[0] == "#" && tokens[1] == "include")
        }
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => {
            tokens[0] == "#include"
                || (tokens.len() >= 2 && tokens[0] == "#" && tokens[1] == "include")
        }
        _ => false,
    }
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
