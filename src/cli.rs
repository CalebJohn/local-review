//! Command-line argument parsing for review mode.
//!
//! Forms:
//!   re                -> None (default staged/unstaged review)
//!   re main           -> SingleRef("main")
//!   re A..B           -> Range { from: A, to: B, three_dot: false }
//!   re A...B          -> Range { from: A, to: B, three_dot: true }
//!   re A B            -> Range { from: A, to: B, three_dot: false }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewArgs {
    SingleRef(String),
    Range { from: String, to: String, three_dot: bool },
}

pub fn usage() -> &'static str {
    "Usage: re [<base> | <A>..<B> | <A>...<B> | <A> <B>]\n\n  re              Review staged and unstaged changes (default)\n  re <base>       Review all changes since diverging from <base> (includes uncommitted)\n  re <A>..<B>     Review changes between commits A and B\n  re <A> <B>      Same as A..B\n  re <A>...<B>    Review changes on B since it diverged from A"
}

/// Parse positional args (argv without the program name).
pub fn parse_args(args: &[String]) -> Result<Option<ReviewArgs>, String> {
    match args {
        [] => Ok(None),
        [single] => {
            if single.starts_with('-') {
                return Err(format!("unknown option '{single}'\n\n{}", usage()));
            }
            if single.contains("..") {
                parse_range(single)
            } else {
                Ok(Some(ReviewArgs::SingleRef(single.clone())))
            }
        }
        [from, to] => {
            if from.starts_with('-') || to.starts_with('-') {
                return Err(format!("unknown option\n\n{}", usage()));
            }
            if from.contains("..") || to.contains("..") {
                return Err(format!(
                    "ambiguous range: use 'A..B' or 'A B', not both\n\n{}",
                    usage()
                ));
            }
            Ok(Some(ReviewArgs::Range {
                from: from.clone(),
                to: to.clone(),
                three_dot: false,
            }))
        }
        _ => Err(format!("too many arguments\n\n{}", usage())),
    }
}

fn parse_range(spec: &str) -> Result<Option<ReviewArgs>, String> {
    let (from, to, three_dot) = if let Some(idx) = spec.find("...") {
        (&spec[..idx], &spec[idx + 3..], true)
    } else if let Some(idx) = spec.find("..") {
        (&spec[..idx], &spec[idx + 2..], false)
    } else {
        unreachable!("caller checked for '..'")
    };
    if from.is_empty() || to.is_empty() {
        return Err(format!("malformed range '{spec}'\n\n{}", usage()));
    }
    if from.contains("..") || to.contains("..") {
        return Err(format!("malformed range '{spec}'\n\n{}", usage()));
    }
    Ok(Some(ReviewArgs::Range {
        from: from.to_string(),
        to: to.to_string(),
        three_dot,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_args_returns_none() {
        assert_eq!(parse_args(&[]), Ok(None));
    }

    #[test]
    fn single_branch_name() {
        assert_eq!(
            parse_args(&args(&["main"])),
            Ok(Some(ReviewArgs::SingleRef("main".into())))
        );
    }

    #[test]
    fn single_revspec_like_head_tilde() {
        assert_eq!(
            parse_args(&args(&["HEAD~1"])),
            Ok(Some(ReviewArgs::SingleRef("HEAD~1".into())))
        );
    }

    #[test]
    fn two_dot_range() {
        assert_eq!(
            parse_args(&args(&["main..feature"])),
            Ok(Some(ReviewArgs::Range {
                from: "main".into(),
                to: "feature".into(),
                three_dot: false,
            }))
        );
    }

    #[test]
    fn three_dot_range() {
        assert_eq!(
            parse_args(&args(&["main...HEAD"])),
            Ok(Some(ReviewArgs::Range {
                from: "main".into(),
                to: "HEAD".into(),
                three_dot: true,
            }))
        );
    }

    #[test]
    fn two_plain_args_become_two_dot_range() {
        assert_eq!(
            parse_args(&args(&["main", "feature"])),
            Ok(Some(ReviewArgs::Range {
                from: "main".into(),
                to: "feature".into(),
                three_dot: false,
            }))
        );
    }

    #[test]
    fn range_missing_right_side_errors() {
        assert!(parse_args(&args(&["main.."])).is_err());
    }

    #[test]
    fn range_missing_left_side_errors() {
        assert!(parse_args(&args(&["..HEAD"])).is_err());
    }

    #[test]
    fn ref_plus_range_token_errors_as_ambiguous() {
        assert!(parse_args(&args(&["main", "A..B"])).is_err());
    }

    #[test]
    fn three_args_error() {
        assert!(parse_args(&args(&["a", "b", "c"])).is_err());
    }

    #[test]
    fn flag_like_arg_errors() {
        assert!(parse_args(&args(&["--verbose"])).is_err());
    }

    #[test]
    fn errors_include_usage() {
        let err = parse_args(&args(&["a", "b", "c"])).unwrap_err();
        assert!(err.contains("Usage: re"), "error should include usage, got: {err}");
    }
}
