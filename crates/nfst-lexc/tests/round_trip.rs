//! Round-trip tests: parse → pretty-print → re-parse → assert AST equal
//! (modulo `Group` wrappers in embedded xre subtrees).
//!
//! The `every_fixture_round_trips` test is the strongest integration
//! signal in the suite — it exercises every section, every entry shape,
//! and the lexc/xre embedding boundary in both directions.

use nfst_lexc::{parse, pretty_print, strip_groups};
use std::fs;
use std::path::Path;

fn round_trip(src: &str) {
    let lhs = parse(src).unwrap_or_else(|e| panic!("first parse of {src:?}: {e:?}"));
    let printed = pretty_print(&lhs);
    let rhs = parse(&printed).unwrap_or_else(|e| {
        panic!("re-parse of pretty-printed {src:?}\n  printed: {printed:?}\n  err: {e:?}")
    });
    let lhs = strip_groups(&lhs);
    let rhs = strip_groups(&rhs);
    assert_eq!(
        lhs.value, rhs.value,
        "round-trip differed for {src:?}\n  printed: {printed:?}",
    );
}

#[test]
fn rt_smallest() {
    round_trip("LEXICON Root\ndog # ;");
}

#[test]
fn rt_definition_with_xre() {
    round_trip("Definitions\nVowel = a | e | i ;\n\nLEXICON Root\nx # ;");
}

#[test]
fn rt_multichar_section() {
    round_trip("Multichar_Symbols +Sg +Pl\n\nLEXICON Root\ndog # ;");
}

#[test]
fn rt_pair_entries() {
    round_trip("LEXICON Root\ncat:dog # ;\nfoo: # ;\n:bar # ;\n: # ;");
}

#[test]
fn rt_xre_block_entry() {
    round_trip("LEXICON Root\n<a b c> # ;");
}

#[test]
fn rt_xre_with_caret_gt() {
    round_trip("LEXICON Root\n<[a|b]^>2> # ;");
}

#[test]
fn rt_gloss() {
    round_trip(
        r#"LEXICON Root
dog Num "the dog" ;"#,
    );
}

#[test]
fn rt_end_marker() {
    round_trip("LEXICON Root\ndog # ;\nEND");
}

#[test]
fn every_fixture_round_trips() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut paths: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "lexc").unwrap_or(false))
        .filter(|e| {
            // Skip xfail fixtures — they're not expected to parse.
            e.path()
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| !n.starts_with("xfail."))
                .unwrap_or(true)
        })
        .map(|e| e.path())
        .collect();
    paths.sort();

    let mut failures: Vec<(std::path::PathBuf, String)> = Vec::new();
    for path in &paths {
        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                failures.push((path.clone(), format!("read: {e}")));
                continue;
            }
        };
        let lhs = match parse(&src) {
            Ok(t) => t,
            Err(e) => {
                failures.push((path.clone(), format!("first parse: {e:?}")));
                continue;
            }
        };
        let printed = pretty_print(&lhs);
        let rhs = match parse(&printed) {
            Ok(t) => t,
            Err(e) => {
                failures.push((
                    path.clone(),
                    format!("re-parse: {e:?}\n  printed:\n{printed}"),
                ));
                continue;
            }
        };
        let lhs = strip_groups(&lhs);
        let rhs = strip_groups(&rhs);
        if lhs.value != rhs.value {
            failures.push((path.clone(), format!("AST diverged\n  printed:\n{printed}")));
        }
    }

    if !failures.is_empty() {
        for (p, e) in &failures {
            eprintln!("FAIL {}: {e}", p.display());
        }
        panic!("{} fixture(s) failed to round-trip", failures.len());
    }
}
