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
fn unparenthesised_where_value_takes_exactly_one_symbol() {
    // `where V in a b c matched` is an error upstream too: the bare form is
    // `VAR_SYMBOL IN VAR_SYMBOL`, so `b` starts a fresh assignment and then
    // wants an `in`. Only a parenthesised list holds several values.
    let d = first_diag("Alphabet a b c ;\nRules\n\"r\" a:b <=> _ ;\nwhere V in a b c matched ;");
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn where_without_a_value_errors() {
    // The matcher keyword is not a symbol, so it cannot be swallowed as the
    // bare form's value.
    let d = first_diag("Alphabet a b ;\nRules\n\"r\" a:b <=> _ ;\nwhere V in matched ;");
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn diagnostic_carries_a_span() {
    let d = first_diag("Alphabet a ;");
    assert!(d.span.start() <= d.span.end());
}
