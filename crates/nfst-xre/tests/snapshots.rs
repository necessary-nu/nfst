//! AST shape assertions per construct. One test per AST variant the parser
//! is expected to emit. Spans are deliberately ignored: `Spanned<T>` compares
//! by value, so test trees can use throwaway spans on inner nodes.

use nfst_syntax::{Span, Spanned};
use nfst_xre::{
    BinaryOp, ContextMark, MappingKind, MappingPair, MappingSide, ReadKind, ReplaceArrow,
    ReplaceContext, ReplaceContexts, ReplaceRule, RestrContext, SpannedXre, SubstituteWhat,
    UnaryOp, XreExpr, parse,
};

fn parsed(src: &str) -> XreExpr {
    parse(src)
        .unwrap_or_else(|e| panic!("parse failed: {e:?}"))
        .value
}

fn sym(s: &str) -> XreExpr {
    XreExpr::Symbol(s.into())
}

fn s(value: XreExpr) -> SpannedXre {
    Spanned::new(value, Span::anonymous(0..0))
}

fn b(value: XreExpr) -> Box<SpannedXre> {
    Box::new(s(value))
}

#[test]
fn epsilon_aliases_all_collapse() {
    assert_eq!(parsed("0"), XreExpr::Epsilon);
    assert_eq!(parsed(r#""""#), XreExpr::Epsilon);
    assert_eq!(parsed("[]"), XreExpr::Epsilon);
}

#[test]
fn any_token() {
    assert_eq!(parsed("?"), XreExpr::Any);
}

#[test]
fn boundary_marker_as_symbol_label() {
    // .#. is lexed as a multichar Symbol with label ".#.".
    assert_eq!(parsed(".#."), sym(".#."));
}

#[test]
fn pair_atom_to_atom() {
    assert_eq!(
        parsed("a:b"),
        XreExpr::Pair {
            upper: b(sym("a")),
            lower: b(sym("b"))
        }
    );
}

#[test]
fn pair_atom_to_curly() {
    assert_eq!(
        parsed("a:{xy}"),
        XreExpr::Pair {
            upper: b(sym("a")),
            lower: b(XreExpr::Curly("xy".into())),
        }
    );
}

#[test]
fn pair_curly_to_atom() {
    assert_eq!(
        parsed("{xy}:a"),
        XreExpr::Pair {
            upper: b(XreExpr::Curly("xy".into())),
            lower: b(sym("a")),
        }
    );
}

#[test]
fn weighted_label() {
    assert_eq!(
        parsed("a::3.5"),
        XreExpr::Weighted {
            expr: b(sym("a")),
            weight: 3.5
        }
    );
}

#[test]
fn weighted_bracketed_expression() {
    let inner = XreExpr::Group(b(XreExpr::Binary(
        BinaryOp::Concatenate,
        b(sym("a")),
        b(sym("b")),
    )));
    assert_eq!(
        parsed("[a b]::2"),
        XreExpr::Weighted {
            expr: Box::new(s(inner)),
            weight: 2.0
        }
    );
}

#[test]
fn group_brackets() {
    let group = XreExpr::Group(b(sym("a")));
    assert_eq!(parsed("[a]"), group);
}

#[test]
fn optional_parens() {
    let opt = XreExpr::Optional(b(sym("a")));
    assert_eq!(parsed("(a)"), opt);
}

#[test]
fn dotted_brackets_empty() {
    assert_eq!(parsed("[..]"), XreExpr::BracketedDotted(None));
}

#[test]
fn dotted_brackets_with_body() {
    assert_eq!(
        parsed("[. a .]"),
        XreExpr::BracketedDotted(Some(b(sym("a"))))
    );
}

#[test]
fn unary_star_plus_reverse_invert_upper_lower() {
    let cases = [
        ("a*", UnaryOp::Star),
        ("a+", UnaryOp::Plus),
        ("a.r", UnaryOp::Reverse),
        ("a.i", UnaryOp::Invert),
        ("a.u", UnaryOp::UpperProject),
        ("a.l", UnaryOp::LowerProject),
    ];
    for (src, op) in cases {
        assert_eq!(parsed(src), XreExpr::Unary(op, b(sym("a"))), "src = {src}");
    }
}

#[test]
fn repeat_n_variants() {
    assert_eq!(parsed("a^3"), XreExpr::RepeatN(b(sym("a")), 3));
    assert_eq!(parsed("a^>3"), XreExpr::RepeatNPlus(b(sym("a")), 3));
    assert_eq!(parsed("a^<3"), XreExpr::RepeatNMinus(b(sym("a")), 3));
    assert_eq!(parsed("a^3,5"), XreExpr::RepeatNToK(b(sym("a")), 3, 5));
    assert_eq!(parsed("a^{3,5}"), XreExpr::RepeatNToK(b(sym("a")), 3, 5));
}

#[test]
fn complement_prefix() {
    assert_eq!(
        parsed("~a"),
        XreExpr::Unary(UnaryOp::Complement, b(sym("a")))
    );
}

#[test]
fn term_complement_prefix() {
    assert_eq!(
        parsed("\\a"),
        XreExpr::Unary(UnaryOp::TermComplement, b(sym("a")))
    );
}

#[test]
fn containment_prefixes() {
    assert_eq!(
        parsed("$a"),
        XreExpr::Unary(UnaryOp::Containment, b(sym("a")))
    );
    assert_eq!(
        parsed("$.a"),
        XreExpr::Unary(UnaryOp::ContainmentOnce, b(sym("a")))
    );
    assert_eq!(
        parsed("$?a"),
        XreExpr::Unary(UnaryOp::ContainmentOpt, b(sym("a")))
    );
}

#[test]
fn containment_with_weight() {
    assert_eq!(
        parsed("$::2 a"),
        XreExpr::ContainmentWithWeight {
            expr: b(sym("a")),
            weight: 2.0,
        }
    );
}

#[test]
fn binary_union_intersect_subtract() {
    let cases = [
        ("a | b", BinaryOp::Union),
        ("a & b", BinaryOp::Intersect),
        ("a - b", BinaryOp::Subtract),
        ("a .P. b", BinaryOp::UpperPriorityUnion),
        ("a .p. b", BinaryOp::LowerPriorityUnion),
    ];
    for (src, op) in cases {
        assert_eq!(
            parsed(src),
            XreExpr::Binary(op, b(sym("a")), b(sym("b"))),
            "src = {src}"
        );
    }
}

#[test]
fn binary_composition_family() {
    let cases = [
        ("a .o. b", BinaryOp::Compose),
        ("a .O. b", BinaryOp::LenientCompose),
        ("a .x. b", BinaryOp::CrossProduct),
        ("a .m>. b", BinaryOp::MergeRight),
        ("a .<m. b", BinaryOp::MergeLeft),
    ];
    for (src, op) in cases {
        assert_eq!(
            parsed(src),
            XreExpr::Binary(op, b(sym("a")), b(sym("b"))),
            "src = {src}"
        );
    }
}

#[test]
fn binary_ignoring_family() {
    let cases = [
        ("a / b", BinaryOp::Ignoring),
        ("a ./. b", BinaryOp::IgnoreInternally),
    ];
    for (src, op) in cases {
        assert_eq!(
            parsed(src),
            XreExpr::Binary(op, b(sym("a")), b(sym("b"))),
            "src = {src}"
        );
    }
}

#[test]
fn binary_before_after_shuffle() {
    let cases = [
        ("a < b", BinaryOp::Before),
        ("a > b", BinaryOp::After),
        ("a <> b", BinaryOp::Shuffle),
    ];
    for (src, op) in cases {
        assert_eq!(
            parsed(src),
            XreExpr::Binary(op, b(sym("a")), b(sym("b"))),
            "src = {src}"
        );
    }
}

#[test]
fn read_file_kinds() {
    let cases = [
        (r#"@bin"x.bin""#, ReadKind::Binary, "x.bin"),
        (r#"@"x.fst""#, ReadKind::Binary, "x.fst"),
        (r#"@txt"x.txt""#, ReadKind::Text, "x.txt"),
        (r#"@stxt"x.stxt""#, ReadKind::Spaced, "x.stxt"),
        (r#"@pl"x.pl""#, ReadKind::Prolog, "x.pl"),
        (r#"@re"x.re""#, ReadKind::Regex, "x.re"),
    ];
    for (src, kind, path) in cases {
        assert_eq!(
            parsed(src),
            XreExpr::ReadFile {
                kind,
                path: path.into()
            },
            "src = {src}"
        );
    }
}

#[test]
fn function_call_zero_args() {
    assert_eq!(
        parsed("Foo()"),
        XreExpr::FunctionCall {
            name: "Foo".into(),
            args: vec![]
        }
    );
}

#[test]
fn function_call_two_args() {
    let expected = XreExpr::FunctionCall {
        name: "Concat".into(),
        args: vec![s(sym("a")), s(sym("b"))],
    };
    assert_eq!(parsed("Concat(a, b)"), expected);
}

fn plain(upper: MappingSide, lower: MappingSide) -> MappingPair {
    MappingPair {
        upper,
        kind: MappingKind::Plain { lower },
    }
}

#[test]
fn replace_right_simple() {
    let mapping = plain(
        MappingSide::Expr(b(sym("a"))),
        MappingSide::Expr(b(sym("b"))),
    );
    let expected = XreExpr::Replace {
        arrow: ReplaceArrow::Right,
        rules: vec![ReplaceRule {
            mappings: vec![mapping],
            contexts: None,
        }],
    };
    assert_eq!(parsed("a -> b"), expected);
}

#[test]
fn replace_with_contexts() {
    let mapping = plain(
        MappingSide::Expr(b(sym("a"))),
        MappingSide::Expr(b(sym("b"))),
    );
    let context = ReplaceContext {
        left: Some(b(sym("c"))),
        right: Some(b(sym("d"))),
    };
    let rule = ReplaceRule {
        mappings: vec![mapping],
        contexts: Some(ReplaceContexts {
            mark: ContextMark::UpperUpper,
            items: vec![context],
        }),
    };
    let expected = XreExpr::Replace {
        arrow: ReplaceArrow::Right,
        rules: vec![rule],
    };
    assert_eq!(parsed("a -> b || c _ d"), expected);
}

#[test]
fn parallel_replace_with_commacomma() {
    let r1 = ReplaceRule {
        mappings: vec![plain(
            MappingSide::Expr(b(sym("a"))),
            MappingSide::Expr(b(sym("b"))),
        )],
        contexts: None,
    };
    let r2 = ReplaceRule {
        mappings: vec![plain(
            MappingSide::Expr(b(sym("c"))),
            MappingSide::Expr(b(sym("d"))),
        )],
        contexts: None,
    };
    let expected = XreExpr::Replace {
        arrow: ReplaceArrow::Right,
        rules: vec![r1, r2],
    };
    assert_eq!(parsed("a -> b ,, c -> d"), expected);
}

#[test]
fn restriction_with_context() {
    let context = RestrContext {
        left: Some(b(sym("b"))),
        right: Some(b(sym("c"))),
    };
    let expected = XreExpr::Restriction {
        body: b(sym("a")),
        contexts: vec![context],
    };
    assert_eq!(parsed("a => b _ c"), expected);
}

#[test]
fn substitute_symbol_form() {
    let expected = XreExpr::Substitute {
        haystack: b(sym("E")),
        what: SubstituteWhat::Symbol {
            needle: "a".into(),
            replacement: vec!["b".into(), "c".into()],
        },
    };
    assert_eq!(parsed("`[ E , a , b c ]"), expected);
}

#[test]
fn substitute_pair_form() {
    let expected = XreExpr::Substitute {
        haystack: b(sym("E")),
        what: SubstituteWhat::Pair {
            from: ("a".into(), "b".into()),
            to: ("c".into(), "d".into()),
        },
    };
    assert_eq!(parsed("`[ E , a:b , c:d ]"), expected);
}

#[test]
fn precedence_concat_binds_tighter_than_union() {
    let expected = XreExpr::Binary(
        BinaryOp::Union,
        b(XreExpr::Binary(
            BinaryOp::Concatenate,
            b(sym("a")),
            b(sym("b")),
        )),
        b(sym("c")),
    );
    assert_eq!(parsed("a b | c"), expected);
}

#[test]
fn precedence_star_binds_tighter_than_concat() {
    let expected = XreExpr::Binary(
        BinaryOp::Concatenate,
        b(sym("a")),
        b(XreExpr::Unary(UnaryOp::Star, b(sym("b")))),
    );
    assert_eq!(parsed("a b*"), expected);
}

#[test]
fn precedence_complement_binds_tighter_than_concat() {
    let expected = XreExpr::Binary(
        BinaryOp::Concatenate,
        b(XreExpr::Unary(UnaryOp::Complement, b(sym("a")))),
        b(sym("b")),
    );
    assert_eq!(parsed("~a b"), expected);
}

// ────────── markup-replace forms (new in this round) ──────────

#[test]
fn replace_with_markup_pre_and_post() {
    // a -> b ... c
    let expected = XreExpr::Replace {
        arrow: ReplaceArrow::Right,
        rules: vec![ReplaceRule {
            mappings: vec![MappingPair {
                upper: MappingSide::Expr(b(sym("a"))),
                kind: MappingKind::Markup {
                    pre: Some(MappingSide::Expr(b(sym("b")))),
                    post: Some(MappingSide::Expr(b(sym("c")))),
                },
            }],
            contexts: None,
        }],
    };
    assert_eq!(parsed("a -> b ... c"), expected);
}

#[test]
fn replace_with_markup_pre_only() {
    // a -> b ...
    let expected = XreExpr::Replace {
        arrow: ReplaceArrow::Right,
        rules: vec![ReplaceRule {
            mappings: vec![MappingPair {
                upper: MappingSide::Expr(b(sym("a"))),
                kind: MappingKind::Markup {
                    pre: Some(MappingSide::Expr(b(sym("b")))),
                    post: None,
                },
            }],
            contexts: None,
        }],
    };
    assert_eq!(parsed("a -> b ..."), expected);
}

#[test]
fn replace_with_markup_post_only() {
    // a -> ... c
    let expected = XreExpr::Replace {
        arrow: ReplaceArrow::Right,
        rules: vec![ReplaceRule {
            mappings: vec![MappingPair {
                upper: MappingSide::Expr(b(sym("a"))),
                kind: MappingKind::Markup {
                    pre: None,
                    post: Some(MappingSide::Expr(b(sym("c")))),
                },
            }],
            contexts: None,
        }],
    };
    assert_eq!(parsed("a -> ... c"), expected);
}

#[test]
fn replace_with_dotted_lhs() {
    // [..] -> a
    let expected = XreExpr::Replace {
        arrow: ReplaceArrow::Right,
        rules: vec![ReplaceRule {
            mappings: vec![plain(
                MappingSide::Dotted(None),
                MappingSide::Expr(b(sym("a"))),
            )],
            contexts: None,
        }],
    };
    assert_eq!(parsed("[..] -> a"), expected);
}

#[test]
fn replace_with_dotted_rhs_with_body() {
    // a -> [. b .]
    let expected = XreExpr::Replace {
        arrow: ReplaceArrow::Right,
        rules: vec![ReplaceRule {
            mappings: vec![plain(
                MappingSide::Expr(b(sym("a"))),
                MappingSide::Dotted(Some(b(sym("b")))),
            )],
            contexts: None,
        }],
    };
    assert_eq!(parsed("a -> [. b .]"), expected);
}

// ────────── lexer edge cases via end-to-end parse ──────────

#[test]
fn bracketed_pound_form_is_a_single_label() {
    // `[.#.]` is a 5-char SYMBOL `.#.` — parses as a single label.
    assert_eq!(parsed("[.#.]"), sym(".#."));
}

#[test]
fn pound_form_inside_real_brackets_unputs_correctly() {
    // `[.#. foo]` — the `[.#.` prefix unputs `.#.`, leaving:
    //   `[` `.#.` `foo` `]`
    // Concatenation grouped in a Group: `[ .#. foo ]`.
    let inner = XreExpr::Binary(BinaryOp::Concatenate, b(sym(".#.")), b(sym("foo")));
    let expected = XreExpr::Group(Box::new(s(inner)));
    assert_eq!(parsed("[.#. foo]"), expected);
}

// ────────── span verification ──────────

#[test]
fn span_on_concatenation_covers_full_expression() {
    let r = parse("a b c").unwrap();
    assert_eq!(r.span.start(), 0);
    assert_eq!(r.span.end(), 5);
}

#[test]
fn span_on_inner_symbol_is_just_that_symbol() {
    // "abc def" — 7 chars total. Inner symbols span 0..3 and 4..7.
    let r = parse("abc def").unwrap();
    if let XreExpr::Binary(BinaryOp::Concatenate, l, _) = &r.value {
        assert_eq!(l.span.start(), 0);
        assert_eq!(l.span.end(), 3);
    } else {
        panic!("expected concat, got {:?}", r.value);
    }
}
