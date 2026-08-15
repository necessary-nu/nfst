//! Per-AST-variant snapshot tests. Each fixture is a tiny `Define`
//! that exercises one specific construct; the test asserts on the
//! expected `PmatchExpr` shape.

use nfst_pmatch::{
    Acceptor, BinaryOp, CaseOp, CaseSide, PmatchExpr, PmatchFile, PmatchStatement, ReadKind,
    UnaryOp, parse,
};

fn parsed(src: &str) -> PmatchFile {
    parse(src)
        .unwrap_or_else(|e| panic!("parse {src:?}: {e:?}"))
        .value
}

fn body(file: &PmatchFile) -> &PmatchExpr {
    match &file.statements[0].value {
        PmatchStatement::Define { body, .. } => &body.value,
        PmatchStatement::DefIns { body, .. } => &body.value,
        PmatchStatement::RegexTop { body } => &body.value,
        PmatchStatement::ListDefinition { body, .. } => &body.value,
        _ => panic!("no body"),
    }
}

#[test]
fn quoted_literal_atom() {
    let f = parsed(r#"Define TOP "foo";"#);
    assert!(matches!(body(&f), PmatchExpr::QuotedLiteral(s) if s == "foo"));
}

#[test]
fn curly_literal_atom() {
    let f = parsed("Define TOP {abc};");
    assert!(matches!(body(&f), PmatchExpr::CurlyLiteral(s) if s == "abc"));
}

#[test]
fn epsilon_atom() {
    let f = parsed("Define TOP 0;");
    assert!(matches!(body(&f), PmatchExpr::Epsilon));
}

#[test]
fn any_atom() {
    let f = parsed("Define TOP ?;");
    assert!(matches!(body(&f), PmatchExpr::Any));
}

#[test]
fn boundary_marker_atom() {
    let f = parsed("Define TOP #;");
    assert!(matches!(body(&f), PmatchExpr::BoundaryMarker));
}

#[test]
fn acceptor_alpha() {
    let f = parsed("Define TOP Alpha;");
    assert!(matches!(body(&f), PmatchExpr::Acceptor(Acceptor::Alpha)));
}

#[test]
fn acceptor_uppercasealpha() {
    let f = parsed("Define TOP UppercaseAlpha;");
    assert!(matches!(
        body(&f),
        PmatchExpr::Acceptor(Acceptor::UppercaseAlpha)
    ));
}

#[test]
fn character_range_atom() {
    let f = parsed(r#"Define TOP "a-z";"#);
    assert!(matches!(
        body(&f),
        PmatchExpr::CharacterRange { from, to } if from == "a" && to == "z"
    ));
}

#[test]
fn star_postfix() {
    let f = parsed("Define TOP a*;");
    assert!(matches!(body(&f), PmatchExpr::Unary(UnaryOp::Star, _)));
}

#[test]
fn complement_prefix() {
    let f = parsed("Define TOP ~a;");
    assert!(matches!(
        body(&f),
        PmatchExpr::Unary(UnaryOp::Complement, _)
    ));
}

#[test]
fn binary_union() {
    let f = parsed("Define TOP a | b;");
    assert!(matches!(
        body(&f),
        PmatchExpr::Binary(BinaryOp::Union, _, _)
    ));
}

#[test]
fn binary_compose() {
    let f = parsed("Define TOP a .o. b;");
    assert!(matches!(
        body(&f),
        PmatchExpr::Binary(BinaryOp::Compose, _, _)
    ));
}

#[test]
fn pair_atom() {
    let f = parsed("Define TOP a:b;");
    assert!(matches!(body(&f), PmatchExpr::Pair { .. }));
}

#[test]
fn group_brackets() {
    let f = parsed("Define TOP [a];");
    assert!(matches!(body(&f), PmatchExpr::Group(_)));
}

#[test]
fn optional_parens() {
    let f = parsed("Define TOP (a);");
    assert!(matches!(body(&f), PmatchExpr::Optional(_)));
}

#[test]
fn dotted_brackets_empty() {
    let f = parsed("Define TOP [..];");
    assert!(matches!(body(&f), PmatchExpr::BracketedDotted(None)));
}

#[test]
fn endtag_call() {
    let f = parsed("Define TOP EndTag(W);");
    assert!(matches!(body(&f), PmatchExpr::EndTag(s) if s == "W"));
}

#[test]
fn capture_call() {
    let f = parsed("Define TOP Capture(word);");
    assert!(matches!(body(&f), PmatchExpr::Capture(s) if s == "word"));
}

#[test]
fn ins_call() {
    let f = parsed("Define TOP Ins(name);");
    assert!(matches!(body(&f), PmatchExpr::Ins(s) if s == "name"));
}

#[test]
fn lit_call() {
    let f = parsed("Define TOP Lit(name);");
    assert!(matches!(body(&f), PmatchExpr::Literal(s) if s == "name"));
}

#[test]
fn counter_call() {
    let f = parsed("Define TOP Counter(words);");
    assert!(matches!(body(&f), PmatchExpr::Counter(s) if s == "words"));
}

#[test]
fn cap_with_no_side() {
    let f = parsed("Define TOP Cap(a);");
    assert!(matches!(
        body(&f),
        PmatchExpr::CaseOp {
            op: CaseOp::Cap,
            side: None,
            ..
        }
    ));
}

#[test]
fn cap_with_upper_side() {
    let f = parsed("Define TOP Cap(a, U);");
    assert!(matches!(
        body(&f),
        PmatchExpr::CaseOp {
            op: CaseOp::Cap,
            side: Some(CaseSide::Upper),
            ..
        }
    ));
}

#[test]
fn lc_context() {
    let f = parsed("Define TOP LC(a);");
    assert!(matches!(body(&f), PmatchExpr::Lc(_)));
}

#[test]
fn or_context() {
    let f = parsed("Define TOP OR(a, b);");
    assert!(matches!(body(&f), PmatchExpr::OrContext(items) if items.len() == 2));
}

#[test]
fn lst_call() {
    let f = parsed("Define TOP Lst({abc});");
    assert!(matches!(body(&f), PmatchExpr::Lst(_)));
}

#[test]
fn sigma_call() {
    let f = parsed("Define TOP Sigma(a);");
    assert!(matches!(body(&f), PmatchExpr::Sigma(_)));
}

#[test]
fn tag_postfix_call() {
    let f = parsed("Define TOP [a].t(W);");
    assert!(matches!(
        body(&f),
        PmatchExpr::Tag { name, .. } if name == "W"
    ));
}

#[test]
fn with_postfix_call() {
    let f = parsed("Define TOP [a].with(case = U);");
    assert!(matches!(
        body(&f),
        PmatchExpr::With { name, value, .. } if name == "case" && value == "U"
    ));
}

#[test]
fn substitute_call() {
    let f = parsed("Define TOP `[ a , b , c ];");
    assert!(matches!(body(&f), PmatchExpr::Substitute(_, _, _)));
}

#[test]
fn uncompose_call() {
    let f = parsed("Define TOP Uncompose(a, b, c);");
    assert!(matches!(body(&f), PmatchExpr::Uncompose(_, _, _)));
}

#[test]
fn user_function_call() {
    let f = parsed("Define TOP MyFunc(a, b);");
    assert!(matches!(
        body(&f),
        PmatchExpr::Call { name, args }
            if name == "MyFunc" && args.len() == 2
    ));
}

#[test]
fn function_definition_with_params() {
    let f = parsed("Define MyTag(name, body) [body EndTag(name)];");
    match &f.statements[0].value {
        PmatchStatement::Define { params, .. } => {
            assert_eq!(params.as_ref().unwrap(), &vec!["name", "body"]);
        }
        _ => panic!(),
    }
}

#[test]
fn defins_statement() {
    let f = parsed("DefIns greeting [{hi}];");
    assert!(matches!(
        &f.statements[0].value,
        PmatchStatement::DefIns { name, .. } if name == "greeting"
    ));
}

#[test]
fn list_statement() {
    let f = parsed("list animals {dog};");
    assert!(matches!(
        &f.statements[0].value,
        PmatchStatement::ListDefinition { name, .. } if name == "animals"
    ));
}

#[test]
fn set_variable_statement() {
    let f = parsed("set need-separators off");
    assert!(matches!(
        &f.statements[0].value,
        PmatchStatement::SetVariable { name, .. } if name == "need-separators"
    ));
}

#[test]
fn regex_top_statement() {
    let f = parsed("regex Alpha;");
    assert!(matches!(
        &f.statements[0].value,
        PmatchStatement::RegexTop { .. }
    ));
}

#[test]
fn read_bin_atom() {
    let f = parsed(r#"Define TOP @"foo.bin";"#);
    assert!(matches!(
        body(&f),
        PmatchExpr::ReadFile { kind: ReadKind::Binary, path } if path == "foo.bin"
    ));
}

#[test]
fn read_lexc_atom() {
    let f = parsed(r#"Define TOP @lexc"foo.lexc";"#);
    assert!(matches!(body(&f), PmatchExpr::ReadLexc(s) if s == "foo.lexc"));
}

#[test]
fn pair_separator_sole_yields_any_pair() {
    // ` : ` standalone → ?:?
    let f = parsed("Define TOP : ;");
    assert!(matches!(body(&f), PmatchExpr::Pair { .. }));
}

#[test]
fn weighted_expression_via_semicolon() {
    let f = parsed("Define TOP a ;::1.5");
    assert!(matches!(
        body(&f),
        PmatchExpr::Weighted { weight, .. } if (*weight - 1.5).abs() < 1e-9
    ));
}

#[test]
fn explode_call() {
    let f = parsed("Define TOP Explode({a}, {b});");
    assert!(matches!(body(&f), PmatchExpr::Explode(items) if items.len() == 2));
}

#[test]
fn implode_call() {
    let f = parsed("Define TOP Implode({a}, {b});");
    assert!(matches!(body(&f), PmatchExpr::Implode(items) if items.len() == 2));
}

#[test]
fn like_with_threshold() {
    let f = parsed("Define TOP Like(a, b)^3;");
    assert!(matches!(
        body(&f),
        PmatchExpr::Like {
            threshold: Some(3),
            unlike: false,
            ..
        }
    ));
}

#[test]
fn unlike_call() {
    let f = parsed("Define TOP Unlike(a, b);");
    assert!(matches!(body(&f), PmatchExpr::Like { unlike: true, .. }));
}

#[test]
fn span_on_top_statement() {
    let f = parse("Define TOP a;").unwrap();
    assert!(f.value.statements[0].span.start() < f.value.statements[0].span.end());
}

// ─────────────────── juxtaposition associativity ───────────────────
// `EXPRESSION7: EXPRESSION7 EXPRESSION7` is ambiguous and bison shifts, so
// juxtaposition chains nest to the right. Concatenation is associative, so
// only tree-shape analyses can observe this — but they do.

/// Flatten a right-nested concatenation chain into its operands.
fn concat_chain(e: &PmatchExpr) -> Vec<&PmatchExpr> {
    let mut out = Vec::new();
    let mut cur = e;
    while let PmatchExpr::Binary(BinaryOp::Concatenate, l, r) = cur {
        out.push(&l.value);
        cur = &r.value;
    }
    out.push(cur);
    out
}

fn symbol_name(e: &PmatchExpr) -> &str {
    match e {
        PmatchExpr::Symbol(s) => s.as_str(),
        other => panic!("expected Symbol, got {other:?}"),
    }
}

fn chain_names(e: &PmatchExpr) -> Vec<&str> {
    concat_chain(e).into_iter().map(symbol_name).collect()
}

#[test]
fn juxtaposition_nests_right() {
    let f = parsed("Define TOP a b c;");
    let PmatchExpr::Binary(BinaryOp::Concatenate, a, rest) = body(&f) else {
        panic!("expected top-level Concatenate, got {:?}", body(&f))
    };
    assert_eq!(symbol_name(&a.value), "a");
    let PmatchExpr::Binary(BinaryOp::Concatenate, b, c) = &rest.value else {
        panic!("expected a Concatenate on the right, got {:?}", rest.value)
    };
    assert_eq!(symbol_name(&b.value), "b");
    assert_eq!(symbol_name(&c.value), "c");
}

#[test]
fn juxtaposition_chain_nests_right_throughout() {
    let f = parsed("Define TOP a b c d e;");
    assert_eq!(chain_names(body(&f)), ["a", "b", "c", "d", "e"]);
}

#[test]
fn juxtaposition_pair_is_unnested() {
    let f = parsed("Define TOP a b;");
    assert_eq!(chain_names(body(&f)), ["a", "b"]);
}

#[test]
fn single_operand_builds_no_concatenation() {
    let f = parsed("Define TOP a;");
    assert!(matches!(body(&f), PmatchExpr::Symbol(s) if s == "a"));
}

#[test]
fn left_context_is_the_left_child_of_the_chain() {
    // The shape a left-concatenation-with-context check keys off: the context
    // is the immediate left child of the top concatenation, not buried in a
    // left-nested spine.
    let f = parsed("Define TOP LC(x) a b;");
    let PmatchExpr::Binary(BinaryOp::Concatenate, left, _) = body(&f) else {
        panic!("expected top-level Concatenate, got {:?}", body(&f))
    };
    assert!(matches!(&left.value, PmatchExpr::Lc(_)));
}

#[test]
fn juxtaposition_binds_tighter_than_union() {
    let f = parsed("Define TOP a b | c d;");
    let PmatchExpr::Binary(BinaryOp::Union, l, r) = body(&f) else {
        panic!("expected Union at the root, got {:?}", body(&f))
    };
    assert_eq!(chain_names(&l.value), ["a", "b"]);
    assert_eq!(chain_names(&r.value), ["c", "d"]);
}

#[test]
fn ignoring_binds_tighter_than_juxtaposition() {
    // expression8 is parsed per-operand, so `b / c` is one operand of the
    // chain rather than the chain being an operand of `/`.
    let f = parsed("Define TOP a b / c;");
    let operands = concat_chain(body(&f));
    assert_eq!(operands.len(), 2);
    assert_eq!(symbol_name(operands[0]), "a");
    assert!(matches!(
        operands[1],
        PmatchExpr::Binary(BinaryOp::Ignoring, _, _)
    ));
}

#[test]
fn postfix_stays_on_its_own_operand() {
    let f = parsed("Define TOP a b* c;");
    let operands = concat_chain(body(&f));
    assert_eq!(operands.len(), 3);
    assert!(matches!(operands[1], PmatchExpr::Unary(UnaryOp::Star, _)));
}

#[test]
fn bracketing_still_forces_left_nesting() {
    // Explicit brackets remain the way to left-nest a chain, and still parse.
    let f = parsed("Define TOP [a b] c;");
    let operands = concat_chain(body(&f));
    assert_eq!(operands.len(), 2);
    assert!(matches!(operands[0], PmatchExpr::Group(_)));
    assert_eq!(symbol_name(operands[1]), "c");
}

#[test]
fn chain_spans_cover_their_own_suffix() {
    let f = parse("Define TOP a b c;").unwrap_or_else(|e| panic!("parse: {e:?}"));
    let PmatchStatement::Define { body, .. } = &f.value.statements[0].value else {
        panic!("expected Define")
    };
    let PmatchExpr::Binary(BinaryOp::Concatenate, _, rest) = &body.value else {
        panic!("expected top-level Concatenate")
    };
    // The inner node starts at `b`, not at the head of the whole chain.
    assert!(rest.span.start() > body.span.start());
    assert_eq!(rest.span.end(), body.span.end());
}

#[test]
fn long_chain_stays_right_nested() {
    // The fold is applied after collecting operands rather than by recursing,
    // so chain length costs the parser no stack. (Dropping the resulting AST
    // still recurses — that is a property of the boxed tree, not the parser.)
    let src = format!("Define TOP {};", vec!["a"; 10_000].join(" "));
    let f = parsed(&src);
    assert_eq!(concat_chain(body(&f)).len(), 10_000);
}
