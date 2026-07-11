//! Pretty-print → re-parse → AST equality (after `strip_groups`)
//! across the entire vendored corpus.

use nfst_xfst::{parse, pretty_print, strip_groups};
use std::path::Path;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn fixtures() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(FIXTURE_DIR).expect("fixtures dir") {
        let p = entry.expect("dirent").path();
        if p.extension().and_then(|x| x.to_str()) == Some("xfst") {
            out.push(p);
        }
    }
    out.sort();
    out
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn round_trip(src: &str) -> Result<(), String> {
    let parsed1 = parse(src).map_err(|e| format!("first parse: {e:?}"))?;
    let printed = pretty_print(&parsed1);
    let parsed2 = parse(&printed).map_err(|e| format!("re-parse: {e:?}\nprinted:\n{printed}"))?;
    let s1 = strip_groups(&parsed1);
    let s2 = strip_groups(&parsed2);
    if s1.value != s2.value {
        return Err(format!("AST diverged.\nprinted:\n{printed}"));
    }
    Ok(())
}

#[test]
fn round_trip_smallest() {
    round_trip("clear ;\nquit").unwrap();
}

#[test]
fn round_trip_define_and_regex() {
    round_trip("define Foo a:b ;\nregex Foo+ ;\nwrite att out.att ;\n").unwrap();
}

#[test]
fn round_trip_substitute() {
    round_trip("regex a ;\nsubstitute symbol A for a\nsubstitute label X:Y for a:b\n").unwrap();
}

#[test]
fn round_trip_apply_inline() {
    round_trip("regex a:b ;\napply up cat\nquit").unwrap();
}

#[test]
fn round_trip_print_with_count() {
    round_trip("regex a ;\nprint words 5 ;\nprint random-words 10 ;\n").unwrap();
}

#[test]
fn round_trip_redirect_chain() {
    round_trip("regex a ;\nprint net > out.txt ;\nwrite att >> log.att ;\n").unwrap();
}

#[test]
fn round_trip_assert_prefix() {
    round_trip("regex a ;\nassert test null ;\n").unwrap();
}

#[test]
fn round_trip_full_corpus() {
    let mut failed: Vec<(String, String)> = Vec::new();
    for path in fixtures() {
        let src = read(&path);
        if let Err(e) = round_trip(&src) {
            failed.push((path.display().to_string(), e));
        }
    }
    assert!(
        failed.is_empty(),
        "{} fixtures failed round-trip:\n{}",
        failed.len(),
        failed
            .iter()
            .take(3)
            .map(|(p, e)| {
                let trunc: String = e.chars().take(400).collect();
                format!("  {p}\n    {trunc}")
            })
            .collect::<Vec<_>>()
            .join("\n")
    );
}
