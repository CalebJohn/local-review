use tree_sitter::Tree;

use super::node_types::{classify_node, expansion_threshold, extract_name, extract_signature, NodeCategory};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AncestorInfo {
    pub expand_to: Option<(u32, u32)>,
    pub header: Option<String>,
}

pub fn parse_source(source: &str, lang_name: &str) -> Option<Tree> {
    let language = crate::syntax::language_for_name(lang_name)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;
    parser.parse(source, None)
}

pub fn ancestor_chain(tree: &Tree, source: &[u8], line_range: (u32, u32), lang: &str) -> AncestorInfo {
    let empty = AncestorInfo { expand_to: None, header: None };

    let start_line = line_range.0;
    let end_line = line_range.1;

    let lookup_line = start_line;
    let start_point = tree_sitter::Point::new(lookup_line as usize, 0);
    let deepest = tree.root_node().descendant_for_point_range(start_point, start_point);
    let deepest = match deepest {
        Some(n) => n,
        None => return empty,
    };

    let mut ancestors: Vec<(NodeCategory, String, Option<String>, u32, u32)> = Vec::new();
    let mut current = Some(deepest);

    while let Some(node) = current {
        if let Some(category) = classify_node(lang, node.kind()) {
            let name = extract_name(&node, source).unwrap_or_default();
            let sig = if matches!(category, NodeCategory::Function | NodeCategory::ClassContainer) {
                extract_signature(&node, source)
            } else {
                None
            };
            let node_start = node.start_position().row as u32;
            let node_end = node.end_position().row as u32;
            ancestors.push((category, name, sig, node_start, node_end));
        }
        current = node.parent();
    }

    ancestors.reverse();

    let header = build_header_breadcrumb(&ancestors);

    let expand_to = find_expansion_target(&ancestors, start_line, end_line);

    AncestorInfo { expand_to, header }
}

fn build_header_breadcrumb(
    ancestors: &[(NodeCategory, String, Option<String>, u32, u32)],
) -> Option<String> {
    let named_scopes: Vec<&(NodeCategory, String, Option<String>, u32, u32)> = ancestors
        .iter()
        .filter(|(cat, _, _, _, _)| matches!(cat, NodeCategory::Function | NodeCategory::ClassContainer))
        .filter(|(_, name, _, _, _)| !name.is_empty())
        .collect();

    if named_scopes.is_empty() {
        return None;
    }

    let parts: Vec<String> = named_scopes
        .iter()
        .enumerate()
        .map(|(i, (_, name, sig, _, _))| {
            if i == named_scopes.len() - 1 {
                if let Some(s) = sig {
                    return s.clone();
                }
            }
            name.clone()
        })
        .collect();

    Some(parts.join(" > "))
}

fn find_expansion_target(
    ancestors: &[(NodeCategory, String, Option<String>, u32, u32)],
    change_start: u32,
    change_end: u32,
) -> Option<(u32, u32)> {
    // Find the innermost ancestor that qualifies for expansion
    let mut best: Option<(u32, u32)> = None;

    for (category, _, _, node_start, node_end) in ancestors.iter().rev() {
        let threshold = expansion_threshold(category);
        if threshold == 0 {
            continue;
        }
        let node_lines = node_end - node_start + 1;
        if node_lines <= threshold {
            // This node qualifies. Check it actually contains the changed lines.
            if *node_start <= change_start && *node_end >= change_end {
                best = Some((*node_start, *node_end));
                break;
            }
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_impl_and_function_header() {
        let src = "impl Foo {\n    fn bar() {\n        let x = 1;\n    }\n}\n";
        let tree = parse_source(src, "rust").unwrap();
        // Line 2 is inside fn bar (0-indexed)
        let info = ancestor_chain(&tree, src.as_bytes(), (2, 2), "rust");
        assert!(info.header.is_some());
        let header = info.header.unwrap();
        assert!(header.contains("Foo"), "header should contain Foo: {header}");
        assert!(header.contains("bar"), "header should contain bar: {header}");
        assert!(header.contains(" > "), "header should use separator: {header}");
    }

    #[test]
    fn test_rust_small_function_expands() {
        let src = "fn small() {\n    let a = 1;\n    let b = 2;\n}\n";
        let tree = parse_source(src, "rust").unwrap();
        // Change at line 1 (0-indexed), inside a 4-line function (lines 0-3)
        let info = ancestor_chain(&tree, src.as_bytes(), (1, 1), "rust");
        assert_eq!(info.expand_to, Some((0, 3)));
    }

    #[test]
    fn test_rust_large_function_no_expansion() {
        let mut lines: Vec<String> = Vec::new();
        lines.push("fn big() {".to_string());
        for i in 0..20 {
            lines.push(format!("    let x{i} = {i};"));
        }
        lines.push("}".to_string());
        let src = lines.join("\n") + "\n";
        let tree = parse_source(&src, "rust").unwrap();
        // Change at line 5 (0-indexed), inside a >15-line function
        let info = ancestor_chain(&tree, src.as_bytes(), (5, 5), "rust");
        assert_eq!(info.expand_to, None);
        // But header should still be populated
        assert!(info.header.is_some());
        assert!(info.header.unwrap().contains("big"));
    }

    #[test]
    fn test_python_class_and_method_header() {
        let src = "class Foo:\n    def bar(self):\n        pass\n";
        let tree = parse_source(src, "python").unwrap();
        let info = ancestor_chain(&tree, src.as_bytes(), (2, 2), "python");
        assert!(info.header.is_some());
        let header = info.header.unwrap();
        assert!(header.contains("Foo"), "header: {header}");
        assert!(header.contains("bar"), "header: {header}");
    }

    #[test]
    fn test_no_parse_returns_empty() {
        let info_opt = parse_source("not valid at all!!!", "unknown_language");
        assert!(info_opt.is_none());
    }

    #[test]
    fn test_no_enclosing_scope() {
        let src = "let x = 1;\nlet y = 2;\n";
        let tree = parse_source(src, "rust").unwrap();
        let info = ancestor_chain(&tree, src.as_bytes(), (0, 0), "rust");
        assert_eq!(info.expand_to, None);
        assert_eq!(info.header, None);
    }

    #[test]
    fn test_parse_source_all_supported_languages() {
        assert!(parse_source("fn main() {}", "rust").is_some());
        assert!(parse_source("x = 1", "python").is_some());
        assert!(parse_source("const x = 1;", "typescript").is_some());
        assert!(parse_source("const x = 1;", "tsx").is_some());
        assert!(parse_source("const x = 1;", "javascript").is_some());
        assert!(parse_source("package main", "go").is_some());
        assert!(parse_source("int x;", "c").is_some());
        assert!(parse_source("int x;", "cpp").is_some());
        assert!(parse_source("{}", "json").is_some());
        assert!(parse_source("a: 1", "yaml").is_some());
        assert!(parse_source("a = 1", "toml").is_some());
    }

    #[test]
    fn test_rust_block_within_threshold_expands() {
        let src = "fn outer() {\n    if true {\n        let a = 1;\n        let b = 2;\n    }\n}\n";
        let tree = parse_source(src, "rust").unwrap();
        // Line 2 is inside the if block (lines 1-4, 4 lines total, <=10 threshold)
        let info = ancestor_chain(&tree, src.as_bytes(), (2, 2), "rust");
        // Should expand to the if block or the function (function is 6 lines, <=15)
        assert!(info.expand_to.is_some());
    }
}
