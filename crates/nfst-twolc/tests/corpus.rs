//! Corpus tests: every `.twolc` fixture must parse cleanly.

use nfst_twolc::parse;
use std::fs;
use std::path::Path;

fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn every_fixture_parses() {
    let dir = fixtures_dir();
    let mut paths: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "twolc").unwrap_or(false))
        .map(|e| e.path())
        .collect();
    paths.sort();

    assert!(
        paths.len() >= 24,
        "expected at least 24 .twolc fixtures, got {}",
        paths.len()
    );

    let mut failures: Vec<(std::path::PathBuf, String)> = Vec::new();
    for path in &paths {
        let src = fs::read_to_string(path).unwrap();
        if let Err(e) = parse(&src) {
            failures.push((path.clone(), format!("{e:?}")));
        }
    }
    if !failures.is_empty() {
        for (p, e) in &failures {
            eprintln!("FAIL {}: {e}", p.display());
        }
        panic!("{} fixture(s) failed to parse", failures.len());
    }
}

#[test]
fn lang_sma_phonology_has_many_rules() {
    let path = fixtures_dir().join("lang-sma__src__fst__morphology__phonology.twolc");
    let src = fs::read_to_string(&path).unwrap();
    let f = parse(&src).unwrap_or_else(|e| panic!("parse: {e:?}"));
    assert!(
        f.value.rules.len() >= 30,
        "lang-sma phonology should have ≥30 rules, got {}",
        f.value.rules.len()
    );
    assert!(!f.value.alphabet.is_empty(), "expected an Alphabet section");
    assert!(!f.value.sets.is_empty(), "expected a Sets section");
    assert!(
        !f.value.definitions.is_empty(),
        "expected a Definitions section"
    );
}

#[test]
fn lang_sme_phonology_uses_where_clauses() {
    let path = fixtures_dir().join("lang-sme__src__fst__morphology__phonology.twolc");
    let src = fs::read_to_string(&path).unwrap();
    let f = parse(&src).unwrap_or_else(|e| panic!("parse: {e:?}"));
    let with_where = f
        .value
        .rules
        .iter()
        .filter(|r| r.value.variables.is_some())
        .count();
    assert!(with_where >= 1, "expected at least one rule with `where`");
}

#[test]
fn omorfi_hyphens_uses_an_unparenthesised_set_name() {
    // Regression: divvun/hfst-rs#3. This generated grammar writes
    // `where VOWEL in Vowels matched`, upstream's `VAR_SYMBOL IN VAR_SYMBOL`
    // production, which we used to reject for want of a `(`.
    let path = fixtures_dir().join("omorfi__src__generated__omorfi-hyphens.twolc");
    let src = fs::read_to_string(&path).unwrap();
    let f = parse(&src).unwrap_or_else(|e| panic!("parse: {e:?}"));
    let vars = f.value.rules[0].value.variables.as_ref().unwrap();
    assert_eq!(vars.len(), 1);
    assert_eq!(vars[0].matcher, nfst_twolc::VarMatcher::Matched);
    assert_eq!(vars[0].assignments[0].name, "VOWEL");
    assert_eq!(vars[0].assignments[0].values, ["Vowels"]);
}

#[test]
fn omorfi_recase_uses_a_bare_symbol_rule_centre() {
    // Regression: divvun/hfst-rs#3 (second report). The rule centre is the
    // single symbol `%{hyph%?%}` with no `:`, upstream's
    // `PAIR: GRAMMAR_SYMBOL_SPACE` production, which we used to reject for
    // want of a `:`. It means the identity pair.
    let path = fixtures_dir().join("omorfi__src__generated__omorfi-recase-any.twolc");
    let src = fs::read_to_string(&path).unwrap();
    let f = parse(&src).unwrap_or_else(|e| panic!("parse: {e:?}"));
    let nfst_twolc::RuleCenter::Pair(pairs) = &f.value.rules[0].value.center else {
        panic!("expected a pair centre");
    };
    assert_eq!(
        pairs,
        &[nfst_twolc::CenterPair {
            upper: nfst_twolc::CenterSide::Symbol("{hyph?}".into()),
            lower: nfst_twolc::CenterSide::Symbol("{hyph?}".into()),
        }]
    );
}

#[test]
fn snippet_where_matched_records_matcher() {
    let path = fixtures_dir().join("snippet-where-matched.twolc");
    let src = fs::read_to_string(&path).unwrap();
    let f = parse(&src).unwrap();
    let vars = f.value.rules[0].value.variables.as_ref().unwrap();
    assert!(
        vars.iter()
            .any(|b| b.matcher == nfst_twolc::VarMatcher::Matched)
    );
}

#[test]
fn snippet_all_arrows_has_four_rules() {
    let path = fixtures_dir().join("snippet-all-arrows.twolc");
    let src = fs::read_to_string(&path).unwrap();
    let f = parse(&src).unwrap();
    assert_eq!(f.value.rules.len(), 4);
}
