//! Per-fixture corpus tests. Each `.xre` file under `tests/fixtures/` is
//! parsed and asserted against a known structural shape so a regression in a
//! single fixture is caught even when the whole-suite parse-success check
//! still passes.
//!
//! These are not exhaustive deep-equality assertions; they spot-check the
//! salient features (top-level operator, mapping count, atom count) that
//! would shift if the parser drifted.

use nfst_xre::{BinaryOp, MappingKind, ReplaceArrow, SpannedXre, UnaryOp, XreExpr, parse_all};
use std::fs;
use std::path::Path;

fn load(name: &str) -> Vec<SpannedXre> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    parse_all(&src).unwrap_or_else(|e| panic!("parse {path:?}: {e:?}"))
}

fn assert_top_concat(expr: &XreExpr, msg: &str) {
    assert!(
        matches!(expr, XreExpr::Binary(BinaryOp::Concatenate, _, _)),
        "{msg}: expected top-level Concatenate, got {expr:?}"
    );
}

fn assert_top_replace(expr: &XreExpr, arrow: ReplaceArrow, mapping_count: usize, msg: &str) {
    match expr {
        XreExpr::Replace { arrow: a, rules } => {
            assert_eq!(*a, arrow, "{msg}: arrow mismatch");
            assert_eq!(
                rules.len(),
                1,
                "{msg}: expected one rule (parallel within), got {}",
                rules.len()
            );
            assert_eq!(
                rules[0].mappings.len(),
                mapping_count,
                "{msg}: mapping count"
            );
            for m in &rules[0].mappings {
                assert!(matches!(m.kind, MappingKind::Plain { .. }));
            }
        }
        other => panic!("{msg}: expected Replace, got {other:?}"),
    }
}

#[test]
fn at_file_quote_foma_concatenates_three() {
    let exprs = load("at_file_quote.foma.xre");
    assert_eq!(exprs.len(), 1);
    assert_top_concat(&exprs[0].value, "at_file_quote.foma");
}

#[test]
fn at_file_quote_openfst_concatenates_three() {
    let exprs = load("at_file_quote.openfst-tropical.xre");
    assert_eq!(exprs.len(), 1);
    assert_top_concat(&exprs[0].value, "at_file_quote.openfst-tropical");
}

#[test]
fn at_file_quote_sfst_concatenates_three() {
    let exprs = load("at_file_quote.sfst.xre");
    assert_eq!(exprs.len(), 1);
    assert_top_concat(&exprs[0].value, "at_file_quote.sfst");
}

#[test]
fn cats_and_dogs_semicolon_has_four_expressions() {
    let exprs = load("cats_and_dogs_semicolon.xre");
    assert_eq!(exprs.len(), 4, "expected 4 semicolon-separated expressions");
    // Each is a concatenation at the top level.
    for (i, e) in exprs.iter().enumerate() {
        assert!(
            matches!(&e.value, XreExpr::Binary(_, _, _)),
            "expression {i} should be a binary expression, got {:?}",
            e.value
        );
    }
}

#[test]
fn cats_and_dogs_no_semicolon_yields_one_expression() {
    // The file is four content lines + a comment line, no semicolons.
    // The four content lines concatenate (with a `|` mid-way) into one
    // expression: ((c a t) | ((d o g) (c:d) … (g:t)::3)).
    let exprs = load("cats_and_dogs.xre");
    assert_eq!(exprs.len(), 1, "expected 1 expression");
    // The top-level operator is Union (the `|` on line 2 is the lowest
    // -precedence operator the file contains).
    assert!(
        matches!(&exprs[0].value, XreExpr::Binary(BinaryOp::Union, _, _)),
        "expected top-level Union, got {:?}",
        exprs[0].value
    );
}

#[test]
fn left_arrow_with_semicolon_comment_one_replace() {
    let exprs = load("left-arrow-with-semicolon-comment.xre");
    assert_eq!(exprs.len(), 1);
    assert_top_replace(
        &exprs[0].value,
        ReplaceArrow::Left,
        1,
        "left-arrow-with-semicolon-comment",
    );
}

#[test]
fn left_arrow_with_semicolon_many_comments_one_replace() {
    let exprs = load("left-arrow-with-semicolon-many-comments.xre");
    assert_eq!(exprs.len(), 1);
    assert_top_replace(
        &exprs[0].value,
        ReplaceArrow::Left,
        1,
        "left-arrow-with-semicolon-many-comments",
    );
}

#[test]
fn not_contains_a_is_complement_of_containment() {
    let exprs = load("not-contains-a.xre");
    assert_eq!(exprs.len(), 1);
    let outer = match &exprs[0].value {
        XreExpr::Unary(UnaryOp::Complement, inner) => inner,
        other => panic!("expected Complement at top, got {other:?}"),
    };
    let inside = match &outer.value {
        XreExpr::Unary(UnaryOp::Containment, inner) => inner,
        other => panic!("expected Containment under Complement, got {other:?}"),
    };
    assert!(
        matches!(&inside.value, XreExpr::Group(_)),
        "expected Group inside Containment, got {:?}",
        inside.value
    );
}

#[test]
fn not_contains_a_with_emptyline_matches_canonical_shape() {
    let canonical = load("not-contains-a.xre");
    let with_comments = load("not-contains-a-comment-emptyline.xre");
    // Comments and blank lines are noise; the AST should be identical.
    assert_eq!(canonical.len(), with_comments.len());
    assert_eq!(canonical[0].value, with_comments[0].value);
}

#[test]
fn parallel_left_arrow_has_three_mappings() {
    // `0 <- a , 0 <- b , 0 <- c` — three parallel mappings.
    let exprs = load("parallel-left-arrow.xre");
    assert_eq!(exprs.len(), 1);
    assert_top_replace(
        &exprs[0].value,
        ReplaceArrow::Left,
        3,
        "parallel-left-arrow",
    );
}

#[test]
fn parallel_left_arrow_multicom_emptyline_has_six_mappings() {
    // The Giellatekno-style fixture deletes `%+Der` through `%+Der5`
    // (six mappings) interleaved with comments and blank lines.
    let exprs = load("parallel-left-arrow-multicom-emptyline.xre");
    assert_eq!(exprs.len(), 1);
    assert_top_replace(
        &exprs[0].value,
        ReplaceArrow::Left,
        6,
        "parallel-left-arrow-multicom-emptyline",
    );
}

#[test]
fn every_fixture_parses_at_least_once() {
    // Catch-all: any new fixture dropped into the fixtures dir at least
    // parses without lex/parse errors.
    let fixtures_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut entries: Vec<_> = fs::read_dir(&fixtures_dir)
        .expect("fixtures dir exists")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "xre").unwrap_or(false))
        .map(|e| e.path())
        .collect();
    entries.sort();

    assert_eq!(
        entries.len(),
        11,
        "expected 11 fixtures, found {}",
        entries.len()
    );

    for path in &entries {
        let src = fs::read_to_string(path).unwrap();
        parse_all(&src).unwrap_or_else(|e| panic!("parse {path:?}: {e:?}"));
    }
}
