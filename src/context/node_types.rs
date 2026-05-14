#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeCategory {
    Function,
    Block,
    ClassContainer,
}

pub fn classify_node(lang: &str, node_kind: &str) -> Option<NodeCategory> {
    match lang {
        "rust" => match node_kind {
            "function_item" => Some(NodeCategory::Function),
            "if_expression" | "match_expression" | "for_expression"
            | "while_expression" | "loop_expression" => Some(NodeCategory::Block),
            "impl_item" | "struct_item" | "enum_item" | "mod_item" => {
                Some(NodeCategory::ClassContainer)
            }
            _ => None,
        },
        "python" => match node_kind {
            "function_definition" => Some(NodeCategory::Function),
            "if_statement" | "for_statement" | "while_statement" | "with_statement"
            | "try_statement" => Some(NodeCategory::Block),
            "class_definition" => Some(NodeCategory::ClassContainer),
            _ => None,
        },
        "typescript" | "tsx" | "javascript" => match node_kind {
            "function_declaration" | "method_definition" | "arrow_function" => {
                Some(NodeCategory::Function)
            }
            "if_statement" | "for_statement" | "while_statement" | "switch_statement"
            | "try_statement" => Some(NodeCategory::Block),
            "class_declaration" => Some(NodeCategory::ClassContainer),
            _ => None,
        },
        "go" => match node_kind {
            "function_declaration" | "method_declaration" => Some(NodeCategory::Function),
            "if_statement" | "for_statement" => Some(NodeCategory::Block),
            "type_declaration" => Some(NodeCategory::ClassContainer),
            _ => None,
        },
        "c" => match node_kind {
            "function_definition" => Some(NodeCategory::Function),
            "if_statement" | "for_statement" | "while_statement" | "switch_statement" => {
                Some(NodeCategory::Block)
            }
            "struct_specifier" => Some(NodeCategory::ClassContainer),
            _ => None,
        },
        "cpp" => match node_kind {
            "function_definition" => Some(NodeCategory::Function),
            "if_statement" | "for_statement" | "while_statement" | "switch_statement" => {
                Some(NodeCategory::Block)
            }
            "struct_specifier" | "class_specifier" => Some(NodeCategory::ClassContainer),
            _ => None,
        },
        "toml" => match node_kind {
            "table" | "table_array_element" => Some(NodeCategory::Block),
            _ => None,
        },
        _ => None,
    }
}

pub fn expansion_threshold(category: &NodeCategory) -> u32 {
    match category {
        NodeCategory::Function => 15,
        NodeCategory::Block => 10,
        NodeCategory::ClassContainer => 0,
    }
}

pub fn extract_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    if let Some(name_node) = node.child_by_field_name("name") {
        return node_text(&name_node, source);
    }
    if let Some(type_node) = node.child_by_field_name("type") {
        return node_text(&type_node, source);
    }
    // TOML tables: extract key text from the first child that is a bare_key or quoted_key,
    // or the bracket-enclosed key text.
    if node.kind() == "table" || node.kind() == "table_array_element" {
        return first_line_of(node, source);
    }
    None
}

pub fn extract_signature(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let body_start = node
        .child_by_field_name("body")
        .or_else(|| node.child_by_field_name("block"))
        .map(|b| b.start_byte());

    let sig_end = match body_start {
        Some(b) => b,
        None => return first_line_of(node, source).map(|s| truncate(&s, 80)),
    };

    let sig_start = node.start_byte();
    if sig_end <= sig_start {
        return None;
    }

    let raw = &source[sig_start..sig_end];
    let sig = String::from_utf8_lossy(raw)
        .lines()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();

    if sig.is_empty() {
        return None;
    }
    Some(truncate(&sig, 80))
}

fn first_line_of(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let start = node.start_byte();
    let line_end = source[start..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|p| start + p)
        .unwrap_or(node.end_byte());
    let text = String::from_utf8_lossy(&source[start..line_end]).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn node_text(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let text = node.utf8_text(source).ok()?.trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max - 1).collect();
        format!("{truncated}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(lang_name: &str, source: &str) -> tree_sitter::Tree {
        let language = crate::syntax::language_for_name(lang_name)
            .unwrap_or_else(|| panic!("unsupported language: {lang_name}"));
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language).unwrap();
        parser.parse(source, None).unwrap()
    }

    fn find_first_node<'a>(
        node: tree_sitter::Node<'a>,
        kind: &str,
    ) -> Option<tree_sitter::Node<'a>> {
        if node.kind() == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_first_node(child, kind) {
                return Some(found);
            }
        }
        None
    }

    // ── classify_node ──

    #[test]
    fn test_classify_rust_function_item() {
        let src = "fn foo() { 1 }";
        let tree = parse("rust", src);
        let node = find_first_node(tree.root_node(), "function_item").unwrap();
        assert_eq!(
            classify_node("rust", node.kind()),
            Some(NodeCategory::Function)
        );
    }

    #[test]
    fn test_classify_rust_impl_item() {
        let src = "struct S; impl S { fn bar(&self) {} }";
        let tree = parse("rust", src);
        let node = find_first_node(tree.root_node(), "impl_item").unwrap();
        assert_eq!(
            classify_node("rust", node.kind()),
            Some(NodeCategory::ClassContainer)
        );
    }

    #[test]
    fn test_classify_rust_if_expression() {
        let src = "fn f() { if true { 1 } else { 2 } }";
        let tree = parse("rust", src);
        let node = find_first_node(tree.root_node(), "if_expression").unwrap();
        assert_eq!(
            classify_node("rust", node.kind()),
            Some(NodeCategory::Block)
        );
    }

    #[test]
    fn test_classify_python_function_definition() {
        let src = "def greet():\n    pass\n";
        let tree = parse("python", src);
        let node = find_first_node(tree.root_node(), "function_definition").unwrap();
        assert_eq!(
            classify_node("python", node.kind()),
            Some(NodeCategory::Function)
        );
    }

    #[test]
    fn test_classify_python_class_definition() {
        let src = "class Foo:\n    pass\n";
        let tree = parse("python", src);
        let node = find_first_node(tree.root_node(), "class_definition").unwrap();
        assert_eq!(
            classify_node("python", node.kind()),
            Some(NodeCategory::ClassContainer)
        );
    }

    #[test]
    fn test_classify_typescript_function_declaration() {
        let src = "function hello(): void { }";
        let tree = parse("typescript", src);
        let node = find_first_node(tree.root_node(), "function_declaration").unwrap();
        assert_eq!(
            classify_node("typescript", node.kind()),
            Some(NodeCategory::Function)
        );
    }

    #[test]
    fn test_classify_javascript_arrow_function() {
        let src = "const f = () => { return 1; };";
        let tree = parse("javascript", src);
        let node = find_first_node(tree.root_node(), "arrow_function").unwrap();
        assert_eq!(
            classify_node("javascript", node.kind()),
            Some(NodeCategory::Function)
        );
    }

    #[test]
    fn test_classify_go_function_declaration() {
        let src = "package main\nfunc hello() { }";
        let tree = parse("go", src);
        let node = find_first_node(tree.root_node(), "function_declaration").unwrap();
        assert_eq!(
            classify_node("go", node.kind()),
            Some(NodeCategory::Function)
        );
    }

    #[test]
    fn test_classify_c_function_definition() {
        let src = "int main() { return 0; }";
        let tree = parse("c", src);
        let node = find_first_node(tree.root_node(), "function_definition").unwrap();
        assert_eq!(
            classify_node("c", node.kind()),
            Some(NodeCategory::Function)
        );
    }

    #[test]
    fn test_classify_cpp_class_specifier() {
        let src = "class Foo { };";
        let tree = parse("cpp", src);
        let node = find_first_node(tree.root_node(), "class_specifier").unwrap();
        assert_eq!(
            classify_node("cpp", node.kind()),
            Some(NodeCategory::ClassContainer)
        );
    }

    #[test]
    fn test_classify_toml_table() {
        let src = "[dependencies]\nfoo = \"1.0\"\n";
        let tree = parse("toml", src);
        let node = find_first_node(tree.root_node(), "table").unwrap();
        assert_eq!(
            classify_node("toml", node.kind()),
            Some(NodeCategory::Block)
        );
    }

    #[test]
    fn test_classify_unknown_node_returns_none() {
        assert_eq!(classify_node("rust", "source_file"), None);
        assert_eq!(classify_node("rust", "let_declaration"), None);
        assert_eq!(classify_node("unknown_lang", "function_item"), None);
    }

    // ── expansion_threshold ──

    #[test]
    fn test_thresholds() {
        assert_eq!(expansion_threshold(&NodeCategory::Function), 15);
        assert_eq!(expansion_threshold(&NodeCategory::Block), 10);
        assert_eq!(expansion_threshold(&NodeCategory::ClassContainer), 0);
    }

    // ── extract_name ──

    #[test]
    fn test_extract_name_rust_function() {
        let src = "fn compute(x: i32) -> bool { true }";
        let tree = parse("rust", src);
        let node = find_first_node(tree.root_node(), "function_item").unwrap();
        assert_eq!(extract_name(&node, src.as_bytes()), Some("compute".into()));
    }

    #[test]
    fn test_extract_name_rust_impl() {
        let src = "struct MyType; impl MyType { }";
        let tree = parse("rust", src);
        let node = find_first_node(tree.root_node(), "impl_item").unwrap();
        assert_eq!(extract_name(&node, src.as_bytes()), Some("MyType".into()));
    }

    #[test]
    fn test_extract_name_python_class() {
        let src = "class Foo:\n    pass\n";
        let tree = parse("python", src);
        let node = find_first_node(tree.root_node(), "class_definition").unwrap();
        assert_eq!(extract_name(&node, src.as_bytes()), Some("Foo".into()));
    }

    #[test]
    fn test_extract_name_python_function() {
        let src = "def bar(self):\n    pass\n";
        let tree = parse("python", src);
        let node = find_first_node(tree.root_node(), "function_definition").unwrap();
        assert_eq!(extract_name(&node, src.as_bytes()), Some("bar".into()));
    }

    #[test]
    fn test_extract_name_toml_table() {
        let src = "[dependencies]\nfoo = \"1.0\"\n";
        let tree = parse("toml", src);
        let node = find_first_node(tree.root_node(), "table").unwrap();
        assert_eq!(
            extract_name(&node, src.as_bytes()),
            Some("[dependencies]".into())
        );
    }

    #[test]
    fn test_extract_name_go_function() {
        let src = "package main\nfunc Hello() { }";
        let tree = parse("go", src);
        let node = find_first_node(tree.root_node(), "function_declaration").unwrap();
        assert_eq!(extract_name(&node, src.as_bytes()), Some("Hello".into()));
    }

    // ── extract_signature ──

    #[test]
    fn test_extract_signature_rust_function() {
        let src = "fn compute(x: i32) -> bool {\n    true\n}";
        let tree = parse("rust", src);
        let node = find_first_node(tree.root_node(), "function_item").unwrap();
        let sig = extract_signature(&node, src.as_bytes()).unwrap();
        assert!(sig.starts_with("fn compute(x: i32) -> bool"));
    }

    #[test]
    fn test_extract_signature_python_function() {
        let src = "def greet(name: str):\n    print(name)\n";
        let tree = parse("python", src);
        let node = find_first_node(tree.root_node(), "function_definition").unwrap();
        let sig = extract_signature(&node, src.as_bytes()).unwrap();
        assert!(sig.contains("def greet(name: str)"));
    }

    #[test]
    fn test_extract_signature_truncates_at_80_chars() {
        let long_params = "a: i32, ".repeat(20);
        let src = format!("fn very_long_function({long_params}) {{\n}}\n");
        let tree = parse("rust", &src);
        let node = find_first_node(tree.root_node(), "function_item").unwrap();
        let sig = extract_signature(&node, src.as_bytes()).unwrap();
        assert!(sig.chars().count() <= 80);
    }

    #[test]
    fn test_extract_signature_toml_table() {
        let src = "[package]\nname = \"foo\"\n";
        let tree = parse("toml", src);
        let node = find_first_node(tree.root_node(), "table").unwrap();
        let sig = extract_signature(&node, src.as_bytes());
        assert!(sig.is_some());
        assert!(sig.unwrap().contains("[package]"));
    }
}
