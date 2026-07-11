//! Per-AST-variant snapshot tests.

use nfst_twolc::{
    AlphabetPair, BinaryOp, RuleCenter, RuleOp, TwolcFile, TwolcRegex, UnaryOp, VarMatcher, parse,
};

fn parsed(src: &str) -> TwolcFile {
    parse(src).unwrap_or_else(|e| panic!("parse: {e:?}")).value
}

#[test]
fn alphabet_section_records_pairs() {
    let f = parsed("Alphabet a b:c ;\nRules\n\"r\" a:b <=> _ ;");
    assert_eq!(f.alphabet.len(), 2);
    assert_eq!(
        f.alphabet[0].value,
        AlphabetPair {
            upper: "a".into(),
            lower: "a".into()
        }
    );
    assert_eq!(
        f.alphabet[1].value,
        AlphabetPair {
            upper: "b".into(),
            lower: "c".into()
        }
    );
}

#[test]
fn rule_with_right_arrow() {
    let f = parsed("Alphabet a b ;\nRules\n\"r\" a:b => _ ;");
    assert_eq!(f.rules[0].value.operator, RuleOp::Right);
}

#[test]
fn rule_with_left_arrow() {
    let f = parsed("Alphabet a b ;\nRules\n\"r\" a:b <= _ ;");
    assert_eq!(f.rules[0].value.operator, RuleOp::Left);
}

#[test]
fn rule_with_left_right_arrow() {
    let f = parsed("Alphabet a b ;\nRules\n\"r\" a:b <=> _ ;");
    assert_eq!(f.rules[0].value.operator, RuleOp::LeftRight);
}

#[test]
fn rule_with_not_left_arrow() {
    let f = parsed("Alphabet a b ;\nRules\n\"r\" a:b /<= _ ;");
    assert_eq!(f.rules[0].value.operator, RuleOp::NotLeft);
}

#[test]
fn rule_center_pair_alternatives() {
    let f = parsed("Alphabet a b c d ;\nRules\n\"r\" [a:b | c:d] <=> _ ;");
    if let RuleCenter::Pair(ps) = &f.rules[0].value.center {
        assert_eq!(ps.len(), 2);
    } else {
        panic!("expected Pair list");
    }
}

#[test]
fn rule_with_except() {
    let f = parsed("Alphabet a b c ;\nRules\n\"r\" a:b => c _ ;\nexcept b _ ;");
    assert_eq!(f.rules[0].value.negative_contexts.len(), 1);
}

#[test]
fn rule_where_matched() {
    let f = parsed(
        "Alphabet a b c d ;\nRules\n\"r\" V:Vy <=> _ ;\nwhere V in (a c) and Vy in (b d) matched ;",
    );
    let blocks = f.rules[0].value.variables.as_ref().unwrap();
    assert!(blocks.iter().any(|b| b.matcher == VarMatcher::Matched));
}

#[test]
fn rule_where_mixed() {
    let f =
        parsed("Alphabet a b c d ;\nRules\n\"r\" X:Y <=> _ ;\nwhere X in (a b) Y in (c d) mixed ;");
    let blocks = f.rules[0].value.variables.as_ref().unwrap();
    assert!(blocks.iter().any(|b| b.matcher == VarMatcher::Mixed));
}

#[test]
fn definitions_section_uses_regex() {
    let f = parsed("Alphabet a b ;\nDefinitions\nFoo = a b ;\nRules\n\"r\" a:b <=> _ ;");
    assert_eq!(f.definitions[0].value.name, "Foo");
    // body is a Concatenate of `a` and `b`
    assert!(matches!(
        f.definitions[0].value.body.value,
        TwolcRegex::Binary(BinaryOp::Concatenate, _, _)
    ));
}

#[test]
fn sets_section_records_members() {
    let f = parsed("Alphabet a b c ;\nSets\nVowel = a e ;\nRules\n\"r\" a:b <=> _ ;");
    assert_eq!(f.sets[0].value.members.len(), 2);
}

#[test]
fn diacritics_section_recorded() {
    let f = parsed("Alphabet a b ;\nDiacritics @P.Foo.Bar@ ;\nRules\n\"r\" a:b <=> _ ;");
    assert_eq!(f.diacritics.len(), 1);
}

#[test]
fn star_postfix_in_context() {
    let f = parsed("Alphabet a b c ;\nRules\n\"r\" a:b => c* _ ;");
    let ctx = &f.rules[0].value.positive_contexts[0];
    assert!(matches!(
        ctx.left.value,
        TwolcRegex::Unary(UnaryOp::Star, _)
    ));
}

#[test]
fn empty_left_context_is_epsilon() {
    let f = parsed("Alphabet a b c ;\nRules\n\"r\" a:b => _ c ;");
    let ctx = &f.rules[0].value.positive_contexts[0];
    assert!(matches!(ctx.left.value, TwolcRegex::Epsilon));
}

#[test]
fn empty_right_context_is_epsilon() {
    let f = parsed("Alphabet a b c ;\nRules\n\"r\" a:b => c _ ;");
    let ctx = &f.rules[0].value.positive_contexts[0];
    assert!(matches!(ctx.right.value, TwolcRegex::Epsilon));
}

#[test]
fn implicit_question_mark_lower() {
    // `name:` followed by whitespace becomes `name:?`.
    let f = parsed("Alphabet a b ;\nRules\n\"r\" a:b => a: _ ;");
    let ctx = &f.rules[0].value.positive_contexts[0];
    if let TwolcRegex::Pair { lower, .. } = &ctx.left.value {
        assert!(matches!(lower.value, TwolcRegex::Any));
    } else {
        panic!("expected Pair");
    }
}

#[test]
fn span_on_top_file() {
    let f = parse("Alphabet a ;\nRules\n\"r\" a:a <=> _ ;").unwrap();
    assert!(f.span.start() < f.span.end());
}

// ───────────────── regex sublanguage coverage ─────────────────

#[test]
fn regex_any_in_context() {
    let f = parsed("Alphabet a b ;\nRules\n\"r\" a:b => ? _ ;");
    let ctx = &f.rules[0].value.positive_contexts[0];
    assert!(matches!(ctx.left.value, TwolcRegex::Any));
}

#[test]
fn regex_explicit_pair_in_context() {
    let f = parsed("Alphabet a b c d ;\nRules\n\"r\" a:b => c:d _ ;");
    let ctx = &f.rules[0].value.positive_contexts[0];
    if let TwolcRegex::Pair { upper, lower } = &ctx.left.value {
        assert!(matches!(upper.value, TwolcRegex::Symbol(ref s) if s == "c"));
        assert!(matches!(lower.value, TwolcRegex::Symbol(ref s) if s == "d"));
    } else {
        panic!("expected Pair, got {:?}", ctx.left.value);
    }
}

#[test]
fn regex_optional_in_context() {
    let f = parsed("Alphabet a b c ;\nRules\n\"r\" a:b => (c) _ ;");
    let ctx = &f.rules[0].value.positive_contexts[0];
    assert!(matches!(ctx.left.value, TwolcRegex::Optional(_)));
}

#[test]
fn regex_group_in_context() {
    let f = parsed("Alphabet a b c d ;\nRules\n\"r\" a:b => [c d] _ ;");
    let ctx = &f.rules[0].value.positive_contexts[0];
    assert!(matches!(ctx.left.value, TwolcRegex::Group(_)));
}

#[test]
fn regex_union_in_definition() {
    let f = parsed("Alphabet a b ;\nDefinitions\nFoo = a | b ;\nRules\n\"r\" a:b <=> _ ;");
    assert!(matches!(
        f.definitions[0].value.body.value,
        TwolcRegex::Binary(BinaryOp::Union, _, _)
    ));
}

#[test]
fn regex_intersect_in_definition() {
    let f = parsed("Alphabet a b ;\nDefinitions\nFoo = a & b ;\nRules\n\"r\" a:b <=> _ ;");
    assert!(matches!(
        f.definitions[0].value.body.value,
        TwolcRegex::Binary(BinaryOp::Intersect, _, _)
    ));
}

#[test]
fn regex_subtract_in_definition() {
    let f = parsed("Alphabet a b ;\nDefinitions\nFoo = a - b ;\nRules\n\"r\" a:b <=> _ ;");
    assert!(matches!(
        f.definitions[0].value.body.value,
        TwolcRegex::Binary(BinaryOp::Subtract, _, _)
    ));
}

#[test]
fn regex_freely_insert_maps_to_ignoring() {
    let f = parsed("Alphabet a b ;\nDefinitions\nFoo = a / b ;\nRules\n\"r\" a:b <=> _ ;");
    assert!(matches!(
        f.definitions[0].value.body.value,
        TwolcRegex::Binary(BinaryOp::Ignoring, _, _)
    ));
}

#[test]
fn regex_plus_postfix_in_context() {
    let f = parsed("Alphabet a b c ;\nRules\n\"r\" a:b => c+ _ ;");
    let ctx = &f.rules[0].value.positive_contexts[0];
    assert!(matches!(
        ctx.left.value,
        TwolcRegex::Unary(UnaryOp::Plus, _)
    ));
}

#[test]
fn regex_complement_prefix_in_context() {
    let f = parsed("Alphabet a b c ;\nRules\n\"r\" a:b => ~c _ ;");
    let ctx = &f.rules[0].value.positive_contexts[0];
    assert!(matches!(
        ctx.left.value,
        TwolcRegex::Unary(UnaryOp::Complement, _)
    ));
}

#[test]
fn regex_term_complement_prefix_in_context() {
    let f = parsed("Alphabet a b c ;\nRules\n\"r\" a:b => \\c _ ;");
    let ctx = &f.rules[0].value.positive_contexts[0];
    assert!(matches!(
        ctx.left.value,
        TwolcRegex::Unary(UnaryOp::TermComplement, _)
    ));
}

#[test]
fn regex_containment_prefix_in_context() {
    let f = parsed("Alphabet a b c ;\nRules\n\"r\" a:b => $c _ ;");
    let ctx = &f.rules[0].value.positive_contexts[0];
    assert!(matches!(
        ctx.left.value,
        TwolcRegex::Unary(UnaryOp::Containment, _)
    ));
}

#[test]
fn regex_containment_once_prefix_in_context() {
    let f = parsed("Alphabet a b c ;\nRules\n\"r\" a:b => $.c _ ;");
    let ctx = &f.rules[0].value.positive_contexts[0];
    assert!(matches!(
        ctx.left.value,
        TwolcRegex::Unary(UnaryOp::ContainmentOnce, _)
    ));
}

#[test]
fn regex_repeat_n_in_context() {
    let f = parsed("Alphabet a b c ;\nRules\n\"r\" a:b => c^3 _ ;");
    let ctx = &f.rules[0].value.positive_contexts[0];
    if let TwolcRegex::RepeatN(_, n) = &ctx.left.value {
        assert_eq!(*n, 3);
    } else {
        panic!("expected RepeatN, got {:?}", ctx.left.value);
    }
}

#[test]
fn regex_repeat_n_to_k_in_context() {
    let f = parsed("Alphabet a b c ;\nRules\n\"r\" a:b => c^2,4 _ ;");
    let ctx = &f.rules[0].value.positive_contexts[0];
    if let TwolcRegex::RepeatNToK(_, n, k) = &ctx.left.value {
        assert_eq!(*n, 2);
        assert_eq!(*k, 4);
    } else {
        panic!("expected RepeatNToK, got {:?}", ctx.left.value);
    }
}

// ───────────────── rule structure coverage ─────────────────

#[test]
fn rule_center_regex_form() {
    let f = parsed("Alphabet a b c ;\nRules\n\"r\" <[ a:b c ]> <=> _ ;");
    assert!(matches!(f.rules[0].value.center, RuleCenter::Regex(_)));
}

#[test]
fn rule_with_multiple_positive_contexts() {
    let f = parsed("Alphabet a b c d ;\nRules\n\"r\" a:b <=> _ c ;\n d _ ;\n");
    assert_eq!(f.rules[0].value.positive_contexts.len(), 2);
}

#[test]
fn rule_where_freely_explicit() {
    let f = parsed(
        "Alphabet a b c d ;\nRules\n\"r\" V:Vy <=> _ ;\nwhere V in (a c) Vy in (b d) freely ;",
    );
    let blocks = f.rules[0].value.variables.as_ref().unwrap();
    assert!(blocks.iter().any(|b| b.matcher == VarMatcher::Freely));
}

#[test]
fn rule_where_default_matcher_is_freely() {
    // Omitted matcher defaults to Freely per upstream pre1.
    let f = parsed("Alphabet a b c d ;\nRules\n\"r\" V:Vy <=> _ ;\nwhere V in (a c) Vy in (b d) ;");
    let blocks = f.rules[0].value.variables.as_ref().unwrap();
    // The single block (no `and` separator) defaults to Freely.
    assert_eq!(blocks.last().unwrap().matcher, VarMatcher::Freely);
}

#[test]
fn rule_where_two_blocks_joined_by_and() {
    let f =
        parsed("Alphabet a b c d ;\nRules\n\"r\" V:Vy <=> _ ;\nwhere V in (a c) and Vy in (b d) ;");
    let blocks = f.rules[0].value.variables.as_ref().unwrap();
    assert_eq!(blocks.len(), 2);
}

#[test]
fn variable_assignment_records_values() {
    let f = parsed("Alphabet a b c d ;\nRules\n\"r\" V:Vy <=> _ ;\nwhere V in (a b c) matched ;");
    let blocks = f.rules[0].value.variables.as_ref().unwrap();
    let assn = &blocks[0].assignments[0];
    assert_eq!(assn.name, "V");
    assert_eq!(assn.values, vec!["a", "b", "c"]);
}

#[test]
fn rule_except_with_multiple_negative_contexts() {
    let f = parsed("Alphabet a b c d ;\nRules\n\"r\" a:b => c _ ;\nexcept b _ ;\n d _ ;\n");
    assert_eq!(f.rules[0].value.negative_contexts.len(), 2);
}

// ───────────────── section structure coverage ─────────────────

#[test]
fn alphabet_with_many_pairs() {
    let f = parsed("Alphabet a b:c d:e f ;\nRules\n\"r\" a:b <=> _ ;");
    assert_eq!(f.alphabet.len(), 4);
    // Single-symbol entries are stored as identity pairs.
    assert_eq!(
        f.alphabet[0].value,
        AlphabetPair {
            upper: "a".into(),
            lower: "a".into()
        }
    );
    assert_eq!(
        f.alphabet[3].value,
        AlphabetPair {
            upper: "f".into(),
            lower: "f".into()
        }
    );
}

#[test]
fn set_with_many_members() {
    let f = parsed("Alphabet a b c d e ;\nSets\nVowel = a e i o u ;\nRules\n\"r\" a:b <=> _ ;");
    assert_eq!(f.sets[0].value.members.len(), 5);
    assert_eq!(f.sets[0].value.members, vec!["a", "e", "i", "o", "u"]);
}

#[test]
fn multiple_diacritics() {
    let f = parsed(
        "Alphabet a b ;\nDiacritics @P.Foo.On@ @P.Foo.Off@ @U.Bar.Baz@ ;\nRules\n\"r\" a:b <=> _ ;",
    );
    assert_eq!(f.diacritics.len(), 3);
}

#[test]
fn multiple_rules_in_file() {
    let f = parsed(
        "Alphabet a b c d ;\nRules\n\
         \"r1\" a:b => _ ;\n\
         \"r2\" c:d <=> _ ;\n\
         \"r3\" a:b /<= _ ;\n",
    );
    assert_eq!(f.rules.len(), 3);
    assert_eq!(f.rules[0].value.name, "r1");
    assert_eq!(f.rules[2].value.name, "r3");
}

#[test]
fn multiple_definitions() {
    let f =
        parsed("Alphabet a b ;\nDefinitions\nFoo = a b ;\nBar = a | b ;\nRules\n\"r\" a:b <=> _ ;");
    assert_eq!(f.definitions.len(), 2);
    assert_eq!(f.definitions[0].value.name, "Foo");
    assert_eq!(f.definitions[1].value.name, "Bar");
}
