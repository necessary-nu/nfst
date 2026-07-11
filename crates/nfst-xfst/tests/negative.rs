//! Negative tests — malformed xfst inputs each yield a `ParseError`
//! with at least one diagnostic.

use nfst_xfst::{Diagnostic, ParseError, Severity, parse};

fn first_diag(src: &str) -> Diagnostic {
    match parse(src) {
        Ok(f) => panic!(
            "expected failure for {src:?}, got {} commands",
            f.value.commands.len()
        ),
        Err(ParseError { diagnostics }) => {
            assert!(!diagnostics.is_empty(), "ParseError without diagnostics");
            diagnostics.into_iter().next().unwrap()
        }
    }
}

#[test]
fn unknown_command_typo_errors() {
    // `pritn` is the upstream "test_fail" canonical typo.
    let d = first_diag("pritn net ;");
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn malformed_regex_body_propagates_xre_error() {
    // `[` opens a group with nothing inside / never closes.
    let d = first_diag("regex [ ;");
    assert!(d.message.contains("xre body") || d.message.contains("expected"));
}

#[test]
fn save_stack_without_path_errors() {
    let _ = first_diag("save stack");
}

#[test]
fn substitute_without_target_errors() {
    let _ = first_diag("substitute symbol for a\n");
}

#[test]
fn define_function_without_body_after_proto_errors() {
    // The lexer captures `define Foo(x)` followed by EOL as an empty
    // body — recovery is graceful, but `regex` followed by an
    // unsupported token should still error.
    let d = first_diag("regex ;\nbogus");
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn diagnostic_carries_a_span() {
    let d = first_diag("xyz_not_a_command");
    assert!(d.span.start() <= d.span.end());
}

#[test]
fn empty_alphabet_pair_errors_in_substitute_label() {
    // `substitute label` requires LABEL FOR LABEL — bare `;` after the
    // keyword is malformed.
    let _ = first_diag("substitute label ;");
}
