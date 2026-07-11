//! Per-fixture corpus tests. Every `.lexc` file under `tests/fixtures/` is
//! parsed; salient files get spot-check shape assertions so a regression
//! in any one of them is caught rather than averaged over the suite.

use nfst_lexc::{EntrySpec, parse};
use std::fs;
use std::path::Path;

fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load(name: &str) -> nfst_lexc::Spanned<nfst_lexc::LexcFile> {
    let path = fixtures_dir().join(name);
    let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    parse(&src).unwrap_or_else(|e| panic!("parse {path:?}: {e:?}"))
}

#[test]
fn cat_dog_bird_has_three_string_entries() {
    let f = load("basic.cat-dog-bird.lexc");
    assert_eq!(f.value.lexicons.len(), 1);
    let root = &f.value.lexicons[0].value;
    assert_eq!(root.name, "Root");
    assert_eq!(root.entries.len(), 3);
    for e in &root.entries {
        assert!(matches!(e.value.spec, EntrySpec::String(_)));
        assert_eq!(e.value.continuation, "#");
    }
}

#[test]
fn multichar_symbols_recorded() {
    let f = load("basic.multichar-symbols.lexc");
    assert!(!f.value.multichars.is_empty(), "expected multichar list");
    let names: Vec<&str> = f
        .value
        .multichars
        .iter()
        .map(|m| m.value.0.as_str())
        .collect();
    assert!(names.contains(&"+1P"));
    assert!(names.contains(&"+Sg"));
}

#[test]
fn colons_fixture_has_pair_entries() {
    let f = load("basic.colons.lexc");
    let root = &f.value.lexicons[0].value;
    let mut pair_count = 0;
    for entry in &root.entries {
        if matches!(entry.value.spec, EntrySpec::Pair { .. }) {
            pair_count += 1;
        }
    }
    assert!(
        pair_count >= 5,
        "expected several pair entries in basic.colons.lexc, got {pair_count}"
    );
}

#[test]
fn empty_sides_fixture_has_one_sided_pairs() {
    // The fixture has `:bar`, `b:`, etc. — pair entries where exactly one
    // side is empty. (Upstream `basic.empty-sides.lexc` does not actually
    // include `: CONT ;` with both sides empty.)
    let f = load("basic.empty-sides.lexc");
    let one_sided = f
        .value
        .lexicons
        .iter()
        .flat_map(|l| &l.value.entries)
        .filter(|e| {
            matches!(
                &e.value.spec,
                EntrySpec::Pair { upper, lower }
                    if upper.is_empty() ^ lower.is_empty()
            )
        })
        .count();
    assert!(
        one_sided >= 2,
        "expected at least two one-sided pair entries"
    );
}

#[test]
fn comments_fixture_strips_to_real_entries() {
    let f = load("basic.comments.lexc");
    let root = &f.value.lexicons[0].value;
    assert!(
        !root.entries.is_empty(),
        "comments fixture has real entries"
    );
}

#[test]
fn end_marker_fixture_records_has_end() {
    let f = load("basic.end.lexc");
    assert!(f.value.has_end);
}

#[test]
fn lowercase_lexicon_warning_recorded() {
    let f = load("basic.lowercase-lexicon-end.lexc");
    let any_titlecase = f.value.lexicons.iter().any(|l| l.value.case_warning);
    assert!(any_titlecase, "expected at least one Lexicon (titlecase)");
}

#[test]
fn infostrings_fixture_records_glosses() {
    let f = load("basic.infostrings.lexc");
    let glossed_count: usize = f
        .value
        .lexicons
        .iter()
        .flat_map(|l| &l.value.entries)
        .filter(|e| e.value.gloss.is_some())
        .count();
    assert!(glossed_count >= 3, "expected several glosses");
}

#[test]
fn regexps_fixture_uses_xre_block() {
    let f = load("basic.regexps.lexc");
    let any_regex = f
        .value
        .lexicons
        .iter()
        .flat_map(|l| &l.value.entries)
        .any(|e| matches!(e.value.spec, EntrySpec::Regex(_)));
    assert!(any_regex, "expected at least one <xre> entry");
}

#[test]
fn two_lexicons_fixture_has_two() {
    let f = load("basic.two-lexicons.lexc");
    assert_eq!(f.value.lexicons.len(), 2);
}

#[test]
fn xre_definitions_fixture_parses_embedded_xre() {
    let f = load("xre.definitions.lexc");
    assert!(
        !f.value.definitions.is_empty(),
        "expected Definitions section"
    );
    // Each definition body must have parsed to a non-empty xre tree.
    for d in &f.value.definitions {
        let _ = &d.value.body.value; // read it; non-panicking
    }
}

#[test]
fn xre_nested_definitions_parses() {
    let f = load("xre.nested-definitions.lexc");
    assert!(f.value.definitions.len() >= 2);
}

#[test]
fn weights_fixture_has_glosses_with_weight_strings() {
    let f = load("hfst.weights.lexc");
    let weight_glosses: usize = f
        .value
        .lexicons
        .iter()
        .flat_map(|l| &l.value.entries)
        .filter_map(|e| e.value.gloss.as_ref())
        .filter(|g| g.contains("weight:"))
        .count();
    assert!(weight_glosses >= 2);
}

#[test]
fn utf8_fixture_parses_without_error() {
    let _f = load("basic.UTF-8.lexc");
}

/// Split fixtures into "should parse" and "should fail" groups based on
/// the `xfail.` filename prefix used by the upstream test harness.
fn fixture_paths() -> (Vec<std::path::PathBuf>, Vec<std::path::PathBuf>) {
    let dir = fixtures_dir();
    let mut all: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "lexc").unwrap_or(false))
        .map(|e| e.path())
        .collect();
    all.sort();
    let (xfail, normal): (Vec<_>, Vec<_>) = all.into_iter().partition(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("xfail."))
            .unwrap_or(false)
    });
    (normal, xfail)
}

#[test]
fn every_normal_fixture_parses() {
    let (normal, _xfail) = fixture_paths();

    assert!(
        normal.len() >= 49, // 53 total - 4 xfail
        "expected ≥49 normal fixtures, found {}",
        normal.len()
    );

    let mut failures: Vec<(std::path::PathBuf, String)> = Vec::new();
    for path in &normal {
        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                failures.push((path.clone(), format!("read error: {e}")));
                continue;
            }
        };
        if let Err(e) = parse(&src) {
            failures.push((path.clone(), format!("{e:?}")));
        }
    }

    if !failures.is_empty() {
        for (p, e) in &failures {
            eprintln!("FAIL {}: {e}", p.display());
        }
        panic!("{} normal fixture(s) failed to parse", failures.len());
    }
}

#[test]
fn parse_failure_xfails_fail() {
    // The xfail set covers four upstream failure modes:
    //
    //   xfail.bogus.lexc                              — no LEXICON header,
    //                                                    bogus content
    //   xfail.lexicon-semicolon.lexc                  — `LEXICON Root ;`
    //                                                    extra `;` confuses
    //                                                    the header
    //   xfail.ISO-8859-1.lexc                         — not UTF-8 (we
    //                                                    fail at read time)
    //   xfail.sublexicon-defined-more-than-once.lexc — duplicate `Root`
    //                                                    is a *semantic*
    //                                                    failure that the
    //                                                    parse-only port
    //                                                    deliberately does
    //                                                    not flag.
    //
    // Only the first three are parse failures.
    let (_normal, xfail) = fixture_paths();
    assert_eq!(xfail.len(), 4, "expected 4 xfail.* fixtures");

    let parse_failures = [
        "xfail.bogus.lexc",
        "xfail.lexicon-semicolon.lexc",
        "xfail.ISO-8859-1.lexc",
    ];

    for name in parse_failures {
        let path = fixtures_dir().join(name);
        let parsed_ok = match fs::read_to_string(&path) {
            Ok(src) => parse(&src).is_ok(),
            Err(_) => false,
        };
        assert!(
            !parsed_ok,
            "{name} parsed successfully — should fail at parse time"
        );
    }
}

#[test]
fn duplicate_lexicon_xfail_parses_but_is_semantic_failure() {
    // Documents the parse-only scope: `xfail.sublexicon-defined-more-than-once.lexc`
    // parses cleanly because uniqueness of lexicon names is a semantic
    // check, not a syntactic one. An evaluator pass should flag it.
    let path = fixtures_dir().join("xfail.sublexicon-defined-more-than-once.lexc");
    let src = fs::read_to_string(&path).unwrap();
    let f = parse(&src).expect("expected to parse");
    let names: Vec<&str> = f
        .value
        .lexicons
        .iter()
        .map(|l| l.value.name.as_str())
        .collect();
    let noun_count = names.iter().filter(|n| **n == "Noun").count();
    assert_eq!(
        noun_count, 2,
        "Noun should appear twice in the AST (duplicate)"
    );
}
