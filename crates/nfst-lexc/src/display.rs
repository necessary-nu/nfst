//! Pretty-printer producing canonical, parseable lexc source from a
//! `Spanned<LexcFile>`. Embedded xre is rendered via `nfst_xre::pretty_print`
//! so a round-trip (lexc → string → lexc → string) preserves the typed
//! tree end-to-end.
//!
//! Like the xre pretty-printer, this one uses defensive escaping: every
//! character that has special meaning in lexc is `%`-escaped on the way
//! out. Round-trip tests strip xre `Group` wrappers via
//! `nfst_xre::strip_groups` so the inevitable `[…]` insertions on the xre
//! side don't break equality.

use crate::ast::{
    Definition, EntrySpec, LexcFile, Lexicon, LexiconEntry, LexiconName, MulticharSymbol,
};
use nfst_syntax::Spanned;
use smol_str::{SmolStr, SmolStrBuilder};
use std::fmt::Write;

pub fn pretty_print(file: &Spanned<LexcFile>) -> SmolStr {
    let mut out = SmolStrBuilder::new();
    write_file(&mut out, &file.value);
    out.finish()
}

fn write_file(out: &mut SmolStrBuilder, file: &LexcFile) {
    if !file.multichars.is_empty() {
        out.push_str("Multichar_Symbols");
        for m in &file.multichars {
            out.push(' ');
            out.push_str(&escape_identifier(&m.value.0));
        }
        out.push('\n');
    }

    if !file.noflags.is_empty() {
        out.push_str("NOFLAGS");
        for n in &file.noflags {
            out.push(' ');
            out.push_str(&escape_identifier(&n.value.0));
        }
        out.push_str(" ;\n");
    }

    if !file.definitions.is_empty() {
        out.push_str("Definitions\n");
        for def in &file.definitions {
            write_definition(out, &def.value);
        }
    }

    for lex in &file.lexicons {
        write_lexicon(out, &lex.value);
    }

    if file.has_end {
        out.push_str("END\n");
    }
}

fn write_definition(out: &mut SmolStrBuilder, def: &Definition) {
    let _ = writeln!(
        out,
        "  {} = {} ;",
        escape_identifier(&def.name),
        nfst_xre::pretty_print(&def.body),
    );
}

fn write_lexicon(out: &mut SmolStrBuilder, lex: &Lexicon) {
    let keyword = if lex.case_warning {
        "Lexicon"
    } else {
        "LEXICON"
    };
    let _ = writeln!(out, "{keyword} {}", escape_identifier(&lex.name));
    for entry in &lex.entries {
        write_entry(out, &entry.value);
    }
    out.push('\n');
}

fn write_entry(out: &mut SmolStrBuilder, e: &LexiconEntry) {
    write_spec(out, &e.spec);
    out.push(' ');
    out.push_str(&escape_identifier(&e.continuation));
    if let Some(g) = &e.gloss {
        let _ = write!(out, " \"{g}\"");
    }
    out.push_str(" ;\n");
}

fn write_spec(out: &mut SmolStrBuilder, spec: &EntrySpec) {
    match spec {
        EntrySpec::Empty => {
            // Nothing — the entry has no spec part.
        }
        EntrySpec::String(s) => {
            out.push_str(&escape_identifier(s));
        }
        EntrySpec::Pair { upper, lower } => {
            out.push_str(&escape_identifier(upper));
            out.push(':');
            out.push_str(&escape_identifier(lower));
        }
        EntrySpec::Regex(xre) => {
            out.push('<');
            out.push_str(&nfst_xre::pretty_print(xre));
            out.push('>');
        }
    }
}

/// Escape any character that is special in the lexc lexer.
///
/// The lexer's NAME_CH set excludes space and `<%!;:"`, plus the `%`
/// escape prefix. To round-trip safely, we prefix any of those with `%`.
fn escape_identifier(s: &str) -> SmolStr {
    let mut out = SmolStrBuilder::new();
    for c in s.chars() {
        if needs_escape(c) {
            out.push('%');
        }
        out.push(c);
    }
    out.finish()
}

fn needs_escape(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\r' | '\n' | '<' | '%' | '!' | ';' | ':' | '"'
    )
}

// ───────────────────────── strip_groups ─────────────────────────

/// Return a `Spanned<LexcFile>` identical in structure to `file`, except
/// every embedded xre subtree has had its `Group` wrappers removed via
/// `nfst_xre::strip_groups`. Used by round-trip tests so the
/// pretty-printer's defensive `[…]` insertions on the xre side do not
/// break equality.
pub fn strip_groups(file: &Spanned<LexcFile>) -> Spanned<LexcFile> {
    let span = file.span.clone();
    let f = &file.value;
    Spanned::new(
        LexcFile {
            multichars: f.multichars.to_vec(),
            noflags: f.noflags.to_vec(),
            definitions: f
                .definitions
                .iter()
                .map(|d| {
                    Spanned::new(
                        Definition {
                            name: d.value.name.clone(),
                            body: nfst_xre::strip_groups(&d.value.body),
                        },
                        d.span.clone(),
                    )
                })
                .collect(),
            lexicons: f
                .lexicons
                .iter()
                .map(|l| {
                    Spanned::new(
                        Lexicon {
                            name: l.value.name.clone(),
                            case_warning: l.value.case_warning,
                            entries: l
                                .value
                                .entries
                                .iter()
                                .map(|e| {
                                    Spanned::new(
                                        LexiconEntry {
                                            spec: strip_groups_spec(&e.value.spec),
                                            continuation: e.value.continuation.clone(),
                                            gloss: e.value.gloss.clone(),
                                        },
                                        e.span.clone(),
                                    )
                                })
                                .collect(),
                        },
                        l.span.clone(),
                    )
                })
                .collect(),
            has_end: f.has_end,
        },
        span,
    )
}

fn strip_groups_spec(spec: &EntrySpec) -> EntrySpec {
    match spec {
        EntrySpec::Empty => EntrySpec::Empty,
        EntrySpec::String(s) => EntrySpec::String(s.clone()),
        EntrySpec::Pair { upper, lower } => EntrySpec::Pair {
            upper: upper.clone(),
            lower: lower.clone(),
        },
        EntrySpec::Regex(xre) => EntrySpec::Regex(nfst_xre::strip_groups(xre)),
    }
}

// keep the unused-import linter happy
#[allow(dead_code)]
fn _unused(_: MulticharSymbol, _: LexiconName) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn round_trip(src: &str) {
        let lhs = parse(src).unwrap_or_else(|e| panic!("first parse of {src:?} failed: {e:?}"));
        let printed = pretty_print(&lhs);
        let rhs = parse(&printed).unwrap_or_else(|e| {
            panic!("re-parse of pretty-printed {src:?}\n  printed: {printed:?}\n  err: {e:?}")
        });
        let lhs = strip_groups(&lhs);
        let rhs = strip_groups(&rhs);
        assert_eq!(
            lhs.value, rhs.value,
            "round-trip differed for {src:?}\n  printed: {printed:?}",
        );
    }

    #[test]
    fn rt_smallest() {
        round_trip("LEXICON Root\ndog # ;");
    }

    #[test]
    fn rt_pair() {
        round_trip("LEXICON Root\ncat:dog # ;");
    }

    #[test]
    fn rt_multichar() {
        round_trip("Multichar_Symbols +Sg +Pl\n\nLEXICON Root\ndog # ;");
    }

    #[test]
    fn rt_definitions_with_xre() {
        round_trip("Definitions\nVowel = a | e | i ;\n\nLEXICON Root\nx # ;");
    }

    #[test]
    fn rt_xre_block_entry() {
        round_trip("LEXICON Root\n<a b c> # ;");
    }

    #[test]
    fn rt_gloss() {
        round_trip(
            r#"LEXICON Root
dog Num "the dog" ;"#,
        );
    }

    #[test]
    fn rt_two_lexicons() {
        round_trip("LEXICON Root\ndog Num ;\nLEXICON Num\n+Sg:s # ;\n");
    }

    #[test]
    fn rt_end() {
        round_trip("LEXICON Root\ndog # ;\nEND");
    }

    #[test]
    fn rt_titlecase_lexicon() {
        round_trip("Lexicon Root\ndog # ;");
    }

    #[test]
    fn rt_empty_pair_sides() {
        round_trip("LEXICON Root\n: # ;");
        round_trip("LEXICON Root\ncat: # ;");
        round_trip("LEXICON Root\n:dog # ;");
    }
}
