//! Negative tests. Each input is intentionally malformed; we want a
//! `ParseError` carrying at least one `Diagnostic`, never a panic, AND the
//! diagnostic's span must point at the token range the user can fix.

use nfst_xre::{Diagnostic, ParseError, Severity, parse};

fn first_diagnostic(src: &str) -> Diagnostic {
    match parse(src) {
        Ok(tree) => panic!("expected failure for {src:?}, got tree {:?}", tree.value),
        Err(ParseError { diagnostics }) => {
            assert!(
                !diagnostics.is_empty(),
                "ParseError without diagnostics for {src:?}"
            );
            diagnostics.into_iter().next().unwrap()
        }
    }
}

fn assert_span(src: &str, expected_range: std::ops::Range<usize>) {
    let d = first_diagnostic(src);
    assert_eq!(
        d.severity,
        Severity::Error,
        "diagnostic for {src:?} not flagged as Error"
    );
    assert_eq!(
        d.span.range, expected_range,
        "diagnostic for {src:?} pointed at the wrong span: got {:?}, want {expected_range:?} (msg: {})",
        d.span.range, d.message
    );
}

#[test]
fn empty_input_yields_error() {
    // Empty input — span collapses to a zero-width point at position 0.
    assert_span("", 0..0);
}

#[test]
fn unmatched_bracket_points_at_eof() {
    // `[ a` — parser consumes `[` and `a`, then fails at EOF expecting `]`.
    // The span is the zero-width point past the last consumed token.
    let d = first_diagnostic("[ a");
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.span.range, 3..3);
}

#[test]
fn unmatched_paren_points_at_eof() {
    let d = first_diagnostic("( a");
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.span.range, 3..3);
}

#[test]
fn dangling_replace_arrow_points_past_arrow() {
    // `a ->` — after consuming `a` and `->`, parser expects an rhs. EOF.
    let d = first_diagnostic("a ->");
    assert_eq!(d.span.range, 4..4);
}

#[test]
fn lex_error_bare_dot_points_at_dot() {
    // `.` is not a valid token start — lex error spanning the dot itself.
    assert_span(".", 0..1);
}

#[test]
fn weight_without_label_points_at_weight() {
    // `::1` is a single Weight token at 0..3; nothing to weight, error.
    let d = first_diagnostic("::1");
    assert_eq!(d.span.range, 0..3);
}

#[test]
fn trailing_garbage_points_at_first_extra_token() {
    // `a ]` — `a` parses, `]` is unexpected at offset 2.
    let d = first_diagnostic("a ]");
    assert_eq!(d.span.range, 2..3);
}

#[test]
fn diagnostic_message_is_nonempty() {
    // Sanity: every error carries a human-readable message.
    let d = first_diagnostic("[ a");
    assert!(!d.message.is_empty());
}
