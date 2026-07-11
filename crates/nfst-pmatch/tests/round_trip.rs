//! Round-trip: parse → pretty-print → re-parse → strip_groups → assert
//! AST equal. The corpus-wide test is the strongest signal in the suite —
//! if it passes, every construct in every fixture survives both directions.

use nfst_pmatch::{parse, pretty_print, strip_groups};
use std::fs;
use std::path::Path;

fn round_trip(src: &str) {
    let lhs = parse(src).unwrap_or_else(|e| panic!("first parse {src:?}: {e:?}"));
    let printed = pretty_print(&lhs);
    let rhs = parse(&printed)
        .unwrap_or_else(|e| panic!("re-parse {src:?}\n  printed: {printed:?}\n  err: {e:?}"));
    let lhs = strip_groups(&lhs);
    let rhs = strip_groups(&rhs);
    assert_eq!(
        lhs.value, rhs.value,
        "round-trip differed for {src:?}\n  printed: {printed:?}",
    );
}

#[test]
fn rt_smallest_define() {
    round_trip(r#"Define TOP "foo";"#);
}

#[test]
fn rt_endtag() {
    round_trip("Define TOP Alpha+ EndTag(W);");
}

#[test]
fn rt_pair() {
    round_trip("Define TOP {a}:{b};");
}

#[test]
fn rt_function_definition() {
    round_trip("Define MyTag(name, body) [body EndTag(name)];");
}

#[test]
fn rt_set_variable() {
    round_trip("set need-separators off");
}

#[test]
fn rt_regex_top() {
    round_trip("regex Alpha+ EndTag(W);");
}

#[test]
fn rt_or_and_contexts() {
    round_trip("Define TOP {a} OR(LC({b}), LC({c})) EndTag(M);");
}

#[test]
fn rt_substitute() {
    round_trip("Define TOP `[ {a} , {b} , {c} ];");
}

#[test]
fn rt_uncompose() {
    round_trip("Define TOP Uncompose({a}, {b}, {c});");
}

#[test]
fn rt_character_range() {
    round_trip(r#"Define TOP "a-z";"#);
}

#[test]
fn rt_acceptors_all() {
    round_trip("Define TOP Alpha | UppercaseAlpha | LowercaseAlpha | Num | Punct | Whitespace;");
}

#[test]
fn rt_explode_implode() {
    round_trip("Define TOP Explode({a}, {b}) | Implode({c}, {d});");
}

#[test]
fn rt_tag_postfix() {
    round_trip("Define TOP [Alpha+].t(W);");
}

#[test]
fn rt_with_postfix() {
    round_trip("Define TOP [Alpha+].with(case = U);");
}

#[test]
fn rt_like_threshold() {
    round_trip("Define TOP Like(hello, world)^3 EndTag(M);");
}

#[test]
fn rt_defins() {
    round_trip("DefIns greeting [{hi} | {hello}];");
}

#[test]
fn rt_list() {
    round_trip("list animals {dog} | {cat};");
}

#[test]
fn rt_lst_exc_sigma() {
    round_trip("Define TOP Lst({a} | {b}) | Exc({c}) | Sigma({d});");
}

#[test]
fn rt_counter() {
    round_trip("Define TOP Alpha+ Counter(words) EndTag(W);");
}

#[test]
fn rt_capture() {
    round_trip("Define TOP Alpha+ Capture(word) EndTag(W);");
}

#[test]
fn every_fixture_round_trips() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    for sub in &["pmscript", "snippets"] {
        let dir = fixtures.join(sub);
        for e in fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()) {
            let p = e.path();
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
            if matches!(ext, "pmscript" | "pmatch") {
                paths.push(p);
            }
        }
    }
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
