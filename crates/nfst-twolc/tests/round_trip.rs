//! Round-trip every twolc fixture: parse → pretty → re-parse → strip_groups
//! → assert AST equality. Real-world phonologies are 1000+ lines each, so
//! any escape/precedence regression surfaces fast.

use nfst_twolc::{parse, pretty_print, strip_groups};
use std::fs;
use std::path::Path;

fn round_trip(src: &str) {
    let lhs = parse(src).unwrap_or_else(|e| panic!("first parse: {e:?}"));
    let printed = pretty_print(&lhs);
    let rhs = parse(&printed)
        .unwrap_or_else(|e| panic!("re-parse failed:\n  printed: {printed}\n  err: {e:?}"));
    let lhs = strip_groups(&lhs);
    let rhs = strip_groups(&rhs);
    assert_eq!(
        lhs.value, rhs.value,
        "round-trip differed\n  printed: {printed}",
    );
}

#[test]
fn rt_smallest() {
    round_trip("Alphabet a b ;\nRules\n\"r\"\na:b => b _ b ;");
}

#[test]
fn rt_all_arrows() {
    round_trip(
        "Alphabet a b c d ;\nRules\n\
         \"r1\" a:b => _ ;\n\
         \"r2\" a:b <= _ ;\n\
         \"r3\" a:b <=> _ ;\n\
         \"r4\" a:b /<= _ ;\n",
    );
}

#[test]
fn rt_with_except() {
    round_trip("Alphabet a b c ;\nRules\n\"r\" a:b => c _ ;\nexcept b _ ;");
}

#[test]
fn rt_with_where_matched() {
    round_trip(
        "Alphabet a b c d ;\nRules\n\"r\" V:Vy <=> _ ;\n\
         where V in (a c) and Vy in (b d) matched ;",
    );
}

#[test]
fn rt_definitions_section() {
    round_trip("Alphabet a b ;\nDefinitions\nFoo = a b ;\nRules\n\"r\" a:b <=> _ ;");
}

#[test]
fn rt_sets_section() {
    round_trip(
        "Alphabet a b c ;\nSets\nVowel = a e ;\nCons = b c ;\n\
         Rules\n\"r\" a:b <=> _ ;",
    );
}

#[test]
fn rt_diacritics_section() {
    round_trip(
        "Alphabet a b ;\nDiacritics @P.Foo.Bar@ ;\n\
         Rules\n\"r\" a:b <=> _ ;",
    );
}

#[test]
fn every_fixture_round_trips() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut paths: Vec<_> = fs::read_dir(&fixtures)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "twolc").unwrap_or(false))
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
                failures.push((path.clone(), format!("re-parse: {e:?}")));
                continue;
            }
        };
        let lhs = strip_groups(&lhs);
        let rhs = strip_groups(&rhs);
        if lhs.value != rhs.value {
            failures.push((path.clone(), "AST diverged".to_string()));
        }
    }

    if !failures.is_empty() {
        for (p, e) in &failures {
            eprintln!("FAIL {}: {e}", p.display());
        }
        panic!("{} fixture(s) failed to round-trip", failures.len());
    }
}
