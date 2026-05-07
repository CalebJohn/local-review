// Canonical comparison utilities for token sequences.
//
// Used in Task 3 to normalize token sequences before comparing old vs new sides
// of a diff hunk.

use std::collections::HashMap;

/// Collect all tokens from the given line numbers, in line order.
///
/// Given a per-line token map (from `extract_tokens`) and a list of 1-based
/// line numbers, returns all tokens concatenated in order.
pub fn collect_tokens(
    token_map: &HashMap<u32, Vec<String>>,
    lines: &[u32],
) -> Vec<String> {
    let mut tokens = Vec::new();
    for &line in lines {
        if let Some(line_tokens) = token_map.get(&line) {
            tokens.extend(line_tokens.iter().cloned());
        }
    }
    tokens
}

/// Strip trailing comma tokens from a token sequence.
///
/// Removes commas that appear:
/// - At the very end of the sequence
/// - Immediately before closing delimiters (`)`, `]`, `}`)
///
/// E.g. `["a", "b", ",", ")"]` → `["a", "b", ")"]`
///      `["a", "b", ","]` → `["a", "b"]`
pub fn strip_trailing_commas(tokens: &[String]) -> Vec<String> {
    let mut result = tokens.to_vec();
    let mut i = result.len();
    while i > 0 {
        if result[i - 1] == "," {
            result.remove(i - 1);
            i -= 1;
        } else if matches!(result[i - 1].as_str(), ")" | "]" | "}") {
            i -= 1;
        } else {
            break;
        }
    }
    result
}

/// Normalize a single token for canonical comparison.
///
/// String literals wrapped in matching quotes are normalized to `S<content>`
/// so that `"foo"` and `'foo'` compare equal.
///
/// All other tokens are returned unchanged.
pub fn normalize_token(token: &str) -> String {
    let bytes = token.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0] as char;
        let last = bytes[bytes.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            let inner = &token[1..bytes.len() - 1];
            return format!("S{}", inner);
        }
    }
    token.to_string()
}

/// Normalize an entire token sequence.
///
/// Applies `normalize_token` to each token in the sequence.
pub fn normalize_tokens(tokens: &[String]) -> Vec<String> {
    tokens.iter().map(|t| normalize_token(t)).collect()
}

/// Build a canonical string from a normalized token sequence.
///
/// Tokens are joined with a single space separator.
pub fn canonical_string(normalized: &[String]) -> String {
    normalized.join(" ")
}

/// Compare two raw token sequences after full normalization.
///
/// Returns `true` if the sequences are equivalent after stripping trailing
/// commas and normalizing string quote styles.
pub fn compare_canonical(old_tokens: &[String], new_tokens: &[String]) -> bool {
    let old = normalize_tokens(&strip_trailing_commas(old_tokens));
    let new = normalize_tokens(&strip_trailing_commas(new_tokens));
    canonical_string(&old) == canonical_string(&new)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── collect_tokens ──────────────────────────────────────────────

    #[test]
    fn test_collect_tokens_single_line() {
        let mut map = HashMap::new();
        map.insert(1, vec!["fn".into(), "main".into(), "(" .into(), ")".into()]);

        let tokens = collect_tokens(&map, &[1]);
        assert_eq!(tokens, vec!["fn", "main", "(", ")"]);
    }

    #[test]
    fn test_collect_tokens_multiple_lines() {
        let mut map = HashMap::new();
        map.insert(1, vec!["let".into(), "x".into()]);
        map.insert(2, vec!["=".into(), "1".into()]);
        map.insert(5, vec!["return".into(), "x".into()]);

        let tokens = collect_tokens(&map, &[1, 2]);
        assert_eq!(tokens, vec!["let", "x", "=", "1"]);
    }

    #[test]
    fn test_collect_tokens_skips_missing_lines() {
        let mut map = HashMap::new();
        map.insert(1, vec!["a".into()]);
        // line 3 not in map

        let tokens = collect_tokens(&map, &[1, 3]);
        assert_eq!(tokens, vec!["a"]);
    }

    #[test]
    fn test_collect_tokens_empty_line_list() {
        let mut map = HashMap::new();
        map.insert(1, vec!["a".into()]);

        let tokens = collect_tokens(&map, &[]);
        assert!(tokens.is_empty());
    }

    // ── strip_trailing_commas ───────────────────────────────────────

    #[test]
    fn test_strip_trailing_commas_removes_one() {
        let tokens = vec!["a".to_string(), "b".to_string(), ",".to_string()];
        let result = strip_trailing_commas(&tokens);
        assert_eq!(result, vec!["a", "b"]);
    }

    #[test]
    fn test_strip_trailing_commas_removes_multiple() {
        let tokens = vec!["a".to_string(), ",".to_string(), ",".to_string()];
        let result = strip_trailing_commas(&tokens);
        assert_eq!(result, vec!["a"]);
    }

    #[test]
    fn test_strip_trailing_commas_no_trailing() {
        let tokens = vec!["a".to_string(), ",".to_string(), "b".to_string()];
        let result = strip_trailing_commas(&tokens);
        assert_eq!(result, vec!["a", ",", "b"]);
    }

    #[test]
    fn test_strip_trailing_commas_empty() {
        let result = strip_trailing_commas(&[]);
        assert!(result.is_empty());
    }

    // ── normalize_token ─────────────────────────────────────────────

    #[test]
    fn test_normalize_token_double_quoted_string() {
        assert_eq!(normalize_token("\"hello\""), "Shello");
    }

    #[test]
    fn test_normalize_token_single_quoted_string() {
        assert_eq!(normalize_token("'hello'"), "Shello");
    }

    #[test]
    fn test_normalize_token_mismatched_quotes_not_normalized() {
        // Mismatched quotes are not valid strings, should pass through
        assert_eq!(normalize_token("\"hello'"), "\"hello'");
    }

    #[test]
    fn test_normalize_token_non_string_unchanged() {
        assert_eq!(normalize_token("fn"), "fn");
        assert_eq!(normalize_token("123"), "123");
        assert_eq!(normalize_token("variable_name"), "variable_name");
    }

    #[test]
    fn test_normalize_token_empty_string_literal() {
        assert_eq!(normalize_token("\"\""), "S");
        assert_eq!(normalize_token("''"), "S");
    }

    #[test]
    fn test_normalize_token_single_char_not_string() {
        assert_eq!(normalize_token("a"), "a");
        assert_eq!(normalize_token("\""), "\"");
    }

    // ── normalize_tokens ────────────────────────────────────────────

    #[test]
    fn test_normalize_tokens_mixed() {
        let tokens = vec!["let".into(), "x".into(), "=".into(), "\"hello\"".into()];
        let result = normalize_tokens(&tokens);
        assert_eq!(result, vec!["let", "x", "=", "Shello"]);
    }

    #[test]
    fn test_normalize_tokens_all_strings_equivalent() {
        let a = normalize_tokens(&["\"foo\"".into()]);
        let b = normalize_tokens(&["'foo'".into()]);
        assert_eq!(a, b, "double and single quoted strings should normalize equally");
    }

    // ── canonical_string ────────────────────────────────────────────

    #[test]
    fn test_canonical_string_joins_with_spaces() {
        let tokens = vec!["fn".into(), "main".into(), "()".into()];
        assert_eq!(canonical_string(&tokens), "fn main ()");
    }

    #[test]
    fn test_canonical_string_empty() {
        assert_eq!(canonical_string(&[]), "");
    }

    #[test]
    fn test_canonical_string_single_token() {
        assert_eq!(canonical_string(&["hello".into()]), "hello");
    }

    // ── compare_canonical ───────────────────────────────────────────

    #[test]
    fn test_compare_canonical_identical() {
        let old = vec!["fn".into(), "main".into(), "()".into()];
        let new = vec!["fn".into(), "main".into(), "()".into()];
        assert!(compare_canonical(&old, &new));
    }

    #[test]
    fn test_compare_canonical_trailing_comma_ignored() {
        let old = vec!["a".into(), "b".into()];
        let new = vec!["a".into(), "b".into(), ",".into()];
        assert!(compare_canonical(&old, &new));
    }

    #[test]
    fn test_compare_canonical_quote_style_ignored() {
        let old = vec!["\"hello\"".into()];
        let new = vec!["'hello'".into()];
        assert!(compare_canonical(&old, &new));
    }

    #[test]
    fn test_compare_canonical_semantic_difference() {
        let old = vec!["let".into(), "x".into(), "=".into(), "1".into()];
        let new = vec!["let".into(), "y".into(), "=".into(), "1".into()];
        assert!(!compare_canonical(&old, &new), "variable rename should be semantic");
    }

    #[test]
    fn test_compare_canonical_empty_sequences() {
        assert!(compare_canonical(&[], &[]));
    }

    #[test]
    fn test_compare_canonical_combined_normalizations() {
        // Trailing comma + quote style change simultaneously
        let old = vec!["call".into(), "(" .into(), "\"arg\"".into(), ")".into()];
        let new = vec!["call".into(), "(" .into(), "'arg'".into(), ")".into(), ",".into()];
        assert!(compare_canonical(&old, &new));
    }
}
