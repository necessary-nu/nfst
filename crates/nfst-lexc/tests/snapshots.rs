//! Per-construct AST shape assertions. One test per AST variant the parser
//! is expected to emit.

use nfst_lexc::{EntrySpec, LexcFile, LexiconName, MulticharSymbol, parse};

fn parsed(src: &str) -> LexcFile {
    parse(src)
        .unwrap_or_else(|e| panic!("parse failed: {e:?}"))
        .value
}

#[test]
fn empty_file_with_just_one_lexicon() {
    let f = parsed("LEXICON Root\ndog # ;");
    assert!(f.multichars.is_empty());
    assert!(f.noflags.is_empty());
    assert!(f.definitions.is_empty());
    assert_eq!(f.lexicons.len(), 1);
    assert!(!f.has_end);
}

#[test]
fn multichar_symbols_section_uppercase() {
    let f = parsed("MULTICHAR_SYMBOLS +Sg +Pl\nLEXICON Root\nx # ;");
    assert_eq!(f.multichars.len(), 2);
    assert_eq!(f.multichars[0].value, MulticharSymbol("+Sg".into()));
}

#[test]
fn alphabets_section_recognized() {
    // Alphabets is the strict-mode variant; we still emit
    // SectionMulticharsStart but with the alphabets flag set.
    // (The parser doesn't store the flag in the AST yet — semantic.)
    let f = parsed("Alphabets a b c\nLEXICON Root\nx # ;");
    assert_eq!(f.multichars.len(), 3);
}

#[test]
fn noflags_section_recognized() {
    let f = parsed("NOFLAGS Foo Bar ;\nLEXICON Root\nx # ;");
    assert_eq!(f.noflags.len(), 2);
    assert_eq!(f.noflags[0].value, LexiconName("Foo".into()));
}

#[test]
fn definitions_section_with_xre_body() {
    let f = parsed("Definitions\nVowel = a | e | i ;\nLEXICON Root\nx # ;");
    assert_eq!(f.definitions.len(), 1);
    assert_eq!(f.definitions[0].value.name, "Vowel");
}

#[test]
fn entry_spec_string() {
    let f = parsed("LEXICON Root\ndog # ;");
    assert!(matches!(
        f.lexicons[0].value.entries[0].value.spec,
        EntrySpec::String(ref s) if s == "dog"
    ));
}

#[test]
fn entry_spec_pair_both_sides() {
    let f = parsed("LEXICON Root\ncat:dog # ;");
    if let EntrySpec::Pair { upper, lower } = &f.lexicons[0].value.entries[0].value.spec {
        assert_eq!(upper, "cat");
        assert_eq!(lower, "dog");
    } else {
        panic!("not a Pair");
    }
}

#[test]
fn entry_spec_pair_upper_only() {
    let f = parsed("LEXICON Root\ncat: # ;");
    if let EntrySpec::Pair { upper, lower } = &f.lexicons[0].value.entries[0].value.spec {
        assert_eq!(upper, "cat");
        assert_eq!(lower, "");
    } else {
        panic!("not a Pair");
    }
}

#[test]
fn entry_spec_pair_lower_only() {
    let f = parsed("LEXICON Root\n:dog # ;");
    if let EntrySpec::Pair { upper, lower } = &f.lexicons[0].value.entries[0].value.spec {
        assert_eq!(upper, "");
        assert_eq!(lower, "dog");
    } else {
        panic!("not a Pair");
    }
}

#[test]
fn entry_spec_pair_both_empty() {
    let f = parsed("LEXICON Root\n: # ;");
    if let EntrySpec::Pair { upper, lower } = &f.lexicons[0].value.entries[0].value.spec {
        assert_eq!(upper, "");
        assert_eq!(lower, "");
    } else {
        panic!("not a Pair");
    }
}

#[test]
fn entry_spec_empty() {
    // `CONT ;` form (no entry text at all).
    let f = parsed("LEXICON Root\n# ;");
    assert!(matches!(
        f.lexicons[0].value.entries[0].value.spec,
        EntrySpec::Empty
    ));
}

#[test]
fn entry_spec_regex() {
    let f = parsed("LEXICON Root\n<a b c> # ;");
    assert!(matches!(
        f.lexicons[0].value.entries[0].value.spec,
        EntrySpec::Regex(_)
    ));
}

#[test]
fn entry_with_gloss_string() {
    let f = parsed(
        r#"LEXICON Root
dog Num "the dog" ;"#,
    );
    assert_eq!(
        f.lexicons[0].value.entries[0].value.gloss.as_deref(),
        Some("the dog")
    );
}

#[test]
fn entry_continuation_recorded() {
    let f = parsed("LEXICON Root\ncat Num ;");
    assert_eq!(f.lexicons[0].value.entries[0].value.continuation, "Num");
}

#[test]
fn end_keyword_sets_has_end() {
    let f = parsed("LEXICON Root\nx # ;\nEND");
    assert!(f.has_end);
}

#[test]
fn lowercase_lexicon_keyword_records_warning() {
    let f = parsed("Lexicon Root\nx # ;");
    assert!(f.lexicons[0].value.case_warning);
}

#[test]
fn uppercase_lexicon_no_warning() {
    let f = parsed("LEXICON Root\nx # ;");
    assert!(!f.lexicons[0].value.case_warning);
}

#[test]
fn multiple_lexicons() {
    let f = parsed("LEXICON Root\nx Num ;\nLEXICON Num\n+Sg # ;\nLEXICON Verb\nv # ;");
    assert_eq!(f.lexicons.len(), 3);
    assert_eq!(f.lexicons[0].value.name, "Root");
    assert_eq!(f.lexicons[1].value.name, "Num");
    assert_eq!(f.lexicons[2].value.name, "Verb");
}

#[test]
fn percent_escape_in_entry_string() {
    // %+N escapes to "+N"
    let f = parsed("LEXICON Root\n%+N # ;");
    assert!(matches!(
        f.lexicons[0].value.entries[0].value.spec,
        EntrySpec::String(ref s) if s == "+N"
    ));
}

#[test]
fn xre_block_with_caret_gt_operator() {
    // The ^>2 operator's `>` must NOT terminate the lexc <…> block.
    let f = parsed("LEXICON Root\n<[a|b]^>2> # ;");
    assert!(matches!(
        f.lexicons[0].value.entries[0].value.spec,
        EntrySpec::Regex(_)
    ));
}

#[test]
fn definition_body_contains_quoted_string_with_semicolon() {
    // Quoted strings inside an xre body can contain `;`; the lexer must
    // not terminate the body early.
    let f = parsed(
        r#"Definitions
Foo = "a;b" ;

LEXICON Root
x # ;"#,
    );
    assert_eq!(f.definitions.len(), 1);
}

#[test]
fn span_on_lexicon_covers_full_block() {
    let src = "LEXICON Root\ndog # ;";
    let f = parse(src).unwrap();
    assert!(f.value.lexicons[0].span.start() < f.value.lexicons[0].span.end());
}
