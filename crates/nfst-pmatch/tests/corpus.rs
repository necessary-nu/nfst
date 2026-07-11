//! Corpus tests: every `.pmscript` from divvun and every curated
//! `.pmatch` snippet must parse cleanly.

use nfst_pmatch::parse;
use std::fs;
use std::path::Path;

fn pmscript_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pmscript")
}
fn snippets_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/snippets")
}

fn collect(dir: &Path, ext: &str) -> Vec<std::path::PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(dir)
        .expect("fixtures dir exists")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == ext).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    paths.sort();
    paths
}

#[test]
fn every_pmscript_parses() {
    let paths = collect(&pmscript_dir(), "pmscript");
    assert!(paths.len() >= 15, "expected ≥15 .pmscript fixtures");

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
        panic!("{} pmscript fixture(s) failed", failures.len());
    }
}

#[test]
fn every_snippet_parses() {
    let paths = collect(&snippets_dir(), "pmatch");
    assert!(paths.len() >= 20, "expected ≥20 snippet fixtures");

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
        panic!("{} snippet fixture(s) failed", failures.len());
    }
}
