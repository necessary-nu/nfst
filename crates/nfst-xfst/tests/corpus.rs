//! Corpus tests — every vendored `.xfst` fixture parses, plus a
//! handful of shape assertions on the more interesting ones.

use nfst_xfst::{XfstCommand, parse};
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

#[test]
fn every_fixture_parses() {
    let mut failed: Vec<(String, String)> = Vec::new();
    for path in fixtures() {
        let src = read(&path);
        if let Err(e) = parse(&src) {
            failed.push((path.display().to_string(), format!("{e:?}")));
        }
    }
    assert!(
        failed.is_empty(),
        "{} fixtures failed to parse:\n{}",
        failed.len(),
        failed
            .iter()
            .take(5)
            .map(|(p, e)| format!("  {p}\n    {e}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn corpus_size_floor() {
    // Sanity: we expect a substantial corpus, not an empty directory.
    let n = fixtures().len();
    assert!(n >= 100, "expected ≥100 fixtures, found {n}");
}

#[test]
fn snippet_regex_and_define_has_define_then_regex() {
    let src = read(
        Path::new(FIXTURE_DIR)
            .join("snippet-regex-and-define.xfst")
            .as_path(),
    );
    let f = parse(&src).expect("parse");
    let cmds: Vec<&XfstCommand> = f.value.commands.iter().map(|c| &c.value).collect();
    assert!(matches!(cmds[0], XfstCommand::Define { .. }));
    assert!(matches!(cmds[1], XfstCommand::Regex(_)));
}

#[test]
fn snippet_apply_heredoc_captures_body() {
    let src = read(
        Path::new(FIXTURE_DIR)
            .join("snippet-apply-heredoc.xfst")
            .as_path(),
    );
    let f = parse(&src).expect("parse");
    let apply = f.value.commands.iter().find_map(|c| match &c.value {
        XfstCommand::Apply(_, Some(b)) => Some(b.clone()),
        _ => None,
    });
    assert!(apply.is_some(), "no Apply with body found");
    let body = apply.unwrap();
    assert!(body.contains("ab"));
    assert!(body.contains("cd"));
}

#[test]
fn snippet_substitute_has_all_three_forms() {
    use nfst_xfst::SubstituteCmd;
    let src = read(
        Path::new(FIXTURE_DIR)
            .join("snippet-substitute.xfst")
            .as_path(),
    );
    let f = parse(&src).expect("parse");
    let mut sym = false;
    let mut lbl = false;
    let mut named = false;
    for c in &f.value.commands {
        if let XfstCommand::Substitute(s) = &c.value {
            match s {
                SubstituteCmd::Symbol { .. } => sym = true,
                SubstituteCmd::Label { .. } => lbl = true,
                SubstituteCmd::Named { .. } => named = true,
            }
        }
    }
    assert!(sym && lbl && named, "missing one of three substitute forms");
}

#[test]
fn snippet_print_family_records_counts() {
    use nfst_xfst::PrintCmd;
    let src = read(
        Path::new(FIXTURE_DIR)
            .join("snippet-print-family.xfst")
            .as_path(),
    );
    let f = parse(&src).expect("parse");
    let words = f.value.commands.iter().find_map(|c| match &c.value {
        XfstCommand::Print(PrintCmd::Words(n)) => Some(*n),
        _ => None,
    });
    assert_eq!(words, Some(Some(5)));
}

#[test]
fn large_script_command_floor() {
    // Pick the heaviest vendored script and assert it has plenty of
    // commands. Catches accidental early-termination regressions.
    let target = fixtures()
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.starts_with("scripts__"))
                .unwrap_or(false)
        })
        .max_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0));
    if let Some(p) = target {
        let src = read(&p);
        let f = parse(&src).expect("parse");
        assert!(
            !f.value.commands.is_empty(),
            "{} produced no commands",
            p.display()
        );
    }
}
