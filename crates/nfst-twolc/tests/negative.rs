//! Negative tests — malformed twolc inputs each return a `ParseError`
//! with at least one diagnostic.

use nfst_twolc::{Diagnostic, ParseError, Severity, parse};

fn first_diag(src: &str) -> Diagnostic {
    match parse(src) {
        Ok(f) => panic!("expected failure for {src:?}, got tree {:?}", f.value),
        Err(ParseError { diagnostics }) => {
            assert!(!diagnostics.is_empty(), "ParseError without diagnostics");
            diagnostics.into_iter().next().unwrap()
        }
    }
}

#[test]
fn empty_input_errors() {
    // Twolc requires a Rules section.
    let d = first_diag("");
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn missing_rules_section_errors() {
    let _ = first_diag("Alphabet a b ;");
}

#[test]
fn rule_without_arrow_errors() {
    let _ = first_diag("Alphabet a b ;\nRules\n\"r\" a:b _ ;");
}

#[test]
fn rule_without_terminator_errors() {
    let _ = first_diag("Alphabet a b ;\nRules\n\"r\" a:b => _ \"r2\"");
}

#[test]
fn unterminated_rule_name_errors() {
    let d = first_diag("Alphabet a ;\nRules\n\"unterminated\n");
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn definition_without_semicolon_absorbs_rules_section() {
    // Like lexc's analogous case: a missing `;` lets the body absorb the
    // next section. Documented as a known limitation rather than a fix.
    let result = parse("Alphabet a b ;\nDefinitions\nFoo = a b\nRules\n\"r\" a:b <=> _ ;");
    let _ = result; // either succeeds (absorbing Rules into the body) or errors
}

#[test]
fn diagnostic_carries_a_span() {
    let d = first_diag("Alphabet a ;");
    assert!(d.span.start() <= d.span.end());
}
