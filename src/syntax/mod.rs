use ratatui::style::Style;

pub mod mapping;
pub mod registry;
pub mod scope;

#[derive(Debug, Clone)]
pub struct StyledSpan {
    pub text: String,
    pub style: Style,
}

pub type StyledLine = Vec<StyledSpan>;

#[derive(Debug, Clone)]
pub struct StyledDiffContent {
    pub lines_by_old_lineno: std::collections::HashMap<u32, StyledLine>,
    pub lines_by_new_lineno: std::collections::HashMap<u32, StyledLine>,
}

pub fn highlight_source(source: &str, extension: Option<&str>) -> Option<Vec<StyledLine>> {
    mapping::highlight_source_inner(source, extension)
}

pub fn language_for_name(lang_name: &str) -> Option<tree_sitter::Language> {
    match lang_name {
        "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
        "typescript" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "javascript" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "python" => Some(tree_sitter_python::LANGUAGE.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        "c" => Some(tree_sitter_c::LANGUAGE.into()),
        "cpp" => Some(tree_sitter_cpp::LANGUAGE.into()),
        "json" => Some(tree_sitter_json::LANGUAGE.into()),
        "yaml" => Some(tree_sitter_yaml::LANGUAGE.into()),
        "toml" => Some(tree_sitter_toml_ng::LANGUAGE.into()),
        _ => None,
    }
}

pub fn lang_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "rs"                          => Some("rust"),
        "ts"                          => Some("typescript"),
        "tsx"                         => Some("tsx"),
        "js" | "jsx" | "mjs" | "cjs"  => Some("javascript"),
        "py" | "pyi"                  => Some("python"),
        "go"                          => Some("go"),
        "c" | "h"                     => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Some("cpp"),
        "json"                        => Some("json"),
        "yaml" | "yml"                => Some("yaml"),
        "toml"                        => Some("toml"),
        _ => None,
    }
}

pub fn build_styled_diff(
    dc: &DiffContent,
    old: Option<&str>,
    new: Option<&str>,
) -> Option<StyledDiffContent> {
    if dc.is_binary { return None; }

    let ext = std::path::Path::new(&dc.path)
        .extension()
        .and_then(|e| e.to_str());

    let old_lines = old.and_then(|s| highlight_source(s, ext));
    let new_lines = new.and_then(|s| highlight_source(s, ext));

    if old_lines.is_none() && new_lines.is_none() {
        return None;
    }

    let mut lines_by_old_lineno: std::collections::HashMap<u32, StyledLine> =
        std::collections::HashMap::new();
    let mut lines_by_new_lineno: std::collections::HashMap<u32, StyledLine> =
        std::collections::HashMap::new();

    if let Some(lines) = old_lines {
        for (i, line) in lines.into_iter().enumerate() {
            lines_by_old_lineno.insert((i as u32) + 1, line);
        }
    }
    if let Some(lines) = new_lines {
        for (i, line) in lines.into_iter().enumerate() {
            lines_by_new_lineno.insert((i as u32) + 1, line);
        }
    }

    Some(StyledDiffContent { lines_by_old_lineno, lines_by_new_lineno })
}

use crate::diff::types::DiffContent;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lang_for_extension_known_cases() {
        assert_eq!(lang_for_extension("rs"), Some("rust"));
        assert_eq!(lang_for_extension("tsx"), Some("tsx"));
        assert_eq!(lang_for_extension("ts"), Some("typescript"));
        assert_eq!(lang_for_extension("yml"), Some("yaml"));
        assert_eq!(lang_for_extension("toml"), Some("toml"));
    }

    #[test]
    fn test_lang_for_extension_unknown_returns_none() {
        assert_eq!(lang_for_extension("xyz"), None);
        assert_eq!(lang_for_extension(""), None);
    }
}