//! Tokens emitted by the lexc lexer. Mirrors `lexc-lexer.ll`'s token set,
//! but normalised: instead of one terminal per flex state, we have a single
//! `Token` enum and the lexer's state determines which token shapes can be
//! produced at each position.

use smol_str::SmolStr;

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    // ──────────────── section starters ────────────────
    /// `Multichar_Symbols` / `MULTICHAR_SYMBOLS` / `Alphabets` /
    /// `ALPHABETS`. `alphabets = true` if the source spelled `Alphabets`,
    /// which the upstream compiler treats as "strict alphabets" mode.
    SectionMulticharsStart {
        alphabets: bool,
    },
    /// `NOFLAGS` / `NoFlags`.
    SectionNoFlagsStart,
    /// `Definitions` / `Declarations` / `DEFINITIONS` / `DECLARATIONS`.
    SectionDefinitionsStart,
    /// `LEXICON name` (titlecase=false) or `Lexicon name` (titlecase=true).
    /// The name has been parsed into a String already.
    LexiconStart {
        name: SmolStr,
        titlecase: bool,
    },
    /// `END` keyword.
    EndKeyword,

    // ──────────────── structural ────────────────
    Equals,
    Semicolon,
    Colon,

    // ──────────────── value-bearing ────────────────
    /// `<xre_body>` block; the body has been extracted (outer `<` and `>`
    /// stripped) and trimmed.
    XreBlock(SmolStr),
    /// `"…"` gloss; outer quotes stripped.
    Quoted(SmolStr),
    /// Generic identifier (NAME_CH+ in lexc terms). Used for multichar
    /// symbols, lexicon names, continuation references, and string
    /// entries — the parser disambiguates by position.
    Identifier(SmolStr),
    /// The body of `name = body ;` in the Definitions section, with `=`
    /// and `;` already consumed by the lexer.
    DefinitionBody(SmolStr),
}
