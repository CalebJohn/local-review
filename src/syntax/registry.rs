use std::collections::HashMap;
use std::sync::OnceLock;
use tree_sitter_highlight::HighlightConfiguration;

pub const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute", "comment", "constant", "constant.builtin", "constructor",
    "function", "function.builtin", "function.method", "keyword", "number",
    "operator", "property", "punctuation", "punctuation.bracket",
    "punctuation.delimiter", "string", "string.special", "tag", "type",
    "type.builtin", "variable", "variable.builtin", "variable.parameter",
];

pub struct HighlightRegistry {
    by_lang: HashMap<&'static str, HighlightConfiguration>,
}

static REGISTRY: OnceLock<HighlightRegistry> = OnceLock::new();

pub fn registry() -> &'static HighlightRegistry {
    REGISTRY.get_or_init(HighlightRegistry::build_all)
}

impl HighlightRegistry {
    fn build_all() -> Self {
        let mut by_lang = HashMap::new();
        by_lang.insert("rust", build("rust",
            tree_sitter_rust::LANGUAGE.into(),
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            ""));
        by_lang.insert("typescript", build("typescript",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "", tree_sitter_typescript::LOCALS_QUERY));
        by_lang.insert("tsx", build("tsx",
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "", tree_sitter_typescript::LOCALS_QUERY));
by_lang.insert("javascript", build("javascript",
                tree_sitter_javascript::LANGUAGE.into(),
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::INJECTIONS_QUERY,
                tree_sitter_javascript::LOCALS_QUERY));
        by_lang.insert("python", build("python",
            tree_sitter_python::LANGUAGE.into(),
            tree_sitter_python::HIGHLIGHTS_QUERY,
            "", ""));
        by_lang.insert("go", build("go",
            tree_sitter_go::LANGUAGE.into(),
            tree_sitter_go::HIGHLIGHTS_QUERY,
            "", ""));
        by_lang.insert("c", build("c",
            tree_sitter_c::LANGUAGE.into(),
            tree_sitter_c::HIGHLIGHT_QUERY,
            "", ""));
        by_lang.insert("cpp", build("cpp",
            tree_sitter_cpp::LANGUAGE.into(),
            tree_sitter_cpp::HIGHLIGHT_QUERY,
            "", ""));
        by_lang.insert("json", build("json",
            tree_sitter_json::LANGUAGE.into(),
            tree_sitter_json::HIGHLIGHTS_QUERY,
            "", ""));
        by_lang.insert("yaml", build("yaml",
            tree_sitter_yaml::LANGUAGE.into(),
            tree_sitter_yaml::HIGHLIGHTS_QUERY,
            "", ""));
        by_lang.insert("toml", build("toml",
            tree_sitter_toml_ng::LANGUAGE.into(),
            tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            "", ""));
        Self { by_lang }
    }

    pub fn get(&self, lang: &str) -> Option<&HighlightConfiguration> {
        self.by_lang.get(lang)
    }
}

fn build(
    name: &'static str,
    lang: tree_sitter::Language,
    highlights: &str,
    injections: &str,
    locals: &str,
) -> HighlightConfiguration {
    let mut cfg = HighlightConfiguration::new(lang, name, highlights, injections, locals)
        .expect("valid highlight configuration");
    cfg.configure(HIGHLIGHT_NAMES);
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter_highlight::{HighlightEvent, Highlighter};

    fn smoke_snippet(lang: &str) -> &'static [u8] {
        match lang {
            "rust"       => b"fn main() {}",
            "typescript" => b"const x: number = 1;",
            "tsx"        => b"const X = () => null;",
            "javascript" => b"const x = 1;",
            "python"     => b"x = 1\n",
            "go"         => b"package main\n",
            "c"          => b"int main(void){return 0;}",
            "cpp"        => b"int main(){return 0;}",
            "json"       => b"{\"a\":1}",
            "yaml"       => b"a: 1\n",
            "toml"       => b"a = 1\n",
            _            => b"",
        }
    }

    #[test]
    fn test_registry_contains_every_expected_language() {
        let reg = registry();
        for lang in ["rust", "typescript", "tsx", "javascript", "python",
                     "go", "c", "cpp", "json", "yaml", "toml"] {
            assert!(reg.get(lang).is_some(), "registry missing language: {lang}");
        }
    }

    #[test]
    fn test_smoke_parse_every_grammar() {
        let reg = registry();
        for lang in ["rust", "typescript", "tsx", "javascript", "python",
                     "go", "c", "cpp", "json", "yaml", "toml"] {
            let cfg = reg.get(lang).unwrap_or_else(|| panic!("no config for {lang}"));
            let mut hl = Highlighter::new();
            let iter = hl
                .highlight(cfg, smoke_snippet(lang), None, |_| None)
                .unwrap_or_else(|e| panic!("highlight failed for {lang}: {e:?}"));
            let mut any_source = false;
            for ev in iter {
                match ev.unwrap_or_else(|e| panic!("event error for {lang}: {e:?}")) {
                    HighlightEvent::Source { .. } => { any_source = true; }
                    _ => {}
                }
            }
            assert!(any_source, "{lang} produced no Source events — grammar ABI broken?");
        }
    }

    #[test]
    fn test_highlight_names_has_23_entries() {
        assert_eq!(HIGHLIGHT_NAMES.len(), 23);
    }
}