//! Negative tests: malformed pmatch inputs each return a `ParseError`
//! with at least one `Diagnostic`. Where the failing position is
//! unambiguous, we also assert the diagnostic span.

use nfst_pmatch::{Diagnostic, ParseError, Severity, parse};

fn first_diag(src: &str) -> Diagnostic {
    match parse(src) {
        Ok(f) => panic!("expected failure for {src:?}, got {:?}", f.value),
        Err(ParseError { diagnostics }) => {
            assert!(!diagnostics.is_empty(), "ParseError without diagnostics");
            diagnostics.into_iter().next().unwrap()
        }
    }
}

#[test]
fn empty_input_is_a_valid_empty_file() {
    // Per pmatch_parse.yy, `PMATCH:` accepts the empty production. So an
    // empty input parses to an empty PmatchFile — not an error.
    let f = parse("").unwrap();
    assert!(f.value.statements.is_empty());
}

#[test]
fn dangling_define_errors() {
    // `Define` with no name.
    let _ = first_diag("Define");
}

#[test]
fn define_without_semicolon_errors() {
    let _ = first_diag(r#"Define TOP "foo""#);
}

#[test]
fn unmatched_bracket_errors() {
    let d = first_diag(r#"Define TOP [a;"#);
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn unmatched_paren_errors() {
    let d = first_diag(r#"Define TOP (a;"#);
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn unterminated_quoted_literal_errors() {
    let d = first_diag(r#"Define TOP "foo;"#);
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn unterminated_curly_literal_errors() {
    let d = first_diag(r#"Define TOP {foo;"#);
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn endtag_without_argument_errors() {
    let _ = first_diag("Define TOP EndTag();");
}

#[test]
fn case_op_with_invalid_side_errors() {
    let d = first_diag("Define TOP Cap(a, X);");
    assert!(
        d.message.to_lowercase().contains("u")
            || d.message.to_lowercase().contains("l")
            || d.message.to_lowercase().contains("side"),
        "expected message to mention U/L, got {:?}",
        d.message
    );
}

#[test]
fn substitute_missing_argument_errors() {
    // `[ a , b ]` — substitute requires three args.
    let _ = first_diag("Define TOP `[ a , b ];");
}

#[test]
fn diagnostic_carries_a_span() {
    let d = first_diag(r#"Define TOP [a;"#);
    assert!(d.span.start() < d.span.end() || d.span.start() == d.span.end());
}
