//! Typed AST for lexc source. Mirrors `lexc-parser.yy`'s shape:
//! a top-level file is a sequence of optional sections (Multichar_Symbols /
//! NoFlags / Definitions) followed by one or more `LEXICON` blocks, with an
//! optional `END` marker.
//!
//! Definition bodies and `<xre> CONT ;` entry shapes embed `nfst_xre`
//! directly: their `body` / `regex` fields are `nfst_xre::SpannedXre`.
//! Round-trips work end-to-end because the `nfst-xre` pretty-printer is
//! callable from the lexc display module.
//!
//! Spans are everywhere via `nfst_syntax::Spanned<T>`. `Spanned`'s
//! `PartialEq` ignores the span, so structural snapshot tests stay
//! readable.

use nfst_syntax::Spanned;
use nfst_xre::SpannedXre;
use smol_str::SmolStr;

/// Top-level lexc source. Empty sections are represented by empty `Vec`s.
#[derive(Clone, Debug, PartialEq)]
pub struct LexcFile {
    pub multichars: Vec<Spanned<MulticharSymbol>>,
    pub noflags: Vec<Spanned<LexiconName>>,
    pub definitions: Vec<Spanned<Definition>>,
    pub lexicons: Vec<Spanned<Lexicon>>,
    /// True if the source contains an explicit `END` marker.
    pub has_end: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MulticharSymbol(pub SmolStr);

#[derive(Clone, Debug, PartialEq)]
pub struct LexiconName(pub SmolStr);

#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    pub name: SmolStr,
    /// Body parsed via `nfst_xre::parse`. Embedding the typed tree (rather
    /// than a raw string) keeps the workspace composable: lexc can analyse
    /// or pretty-print these bodies without re-parsing.
    pub body: SpannedXre,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Lexicon {
    pub name: SmolStr,
    /// True if the source spelled `Lexicon` (titlecase) instead of
    /// `LEXICON`. Upstream emits a warning for this; we preserve the
    /// information so consumers can do the same.
    pub case_warning: bool,
    pub entries: Vec<Spanned<LexiconEntry>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LexiconEntry {
    pub spec: EntrySpec,
    pub continuation: SmolStr,
    /// `"…"` quoted text that follows the continuation. Glosses are also
    /// the C++ encoding for entry-level weights (`"weight: 1"`); semantic
    /// interpretation is the evaluator's job.
    pub gloss: Option<SmolStr>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EntrySpec {
    /// Bare entry: `CONT ;` — the lexicon entry contributes the empty
    /// string (epsilon) on both sides.
    Empty,
    /// `string CONT ;` — single-string entry; identity-encoded by the
    /// upstream compiler (`x:x` for each character).
    String(SmolStr),
    /// `upper:lower CONT ;`. Either side may be the empty string,
    /// representing epsilon. The `:string CONT ;` and `string: CONT ;`
    /// shorthand forms are encoded as a `Pair` with the corresponding
    /// side empty.
    Pair { upper: SmolStr, lower: SmolStr },
    /// `<xre> CONT ;` — embedded xre regex.
    Regex(SpannedXre),
}
