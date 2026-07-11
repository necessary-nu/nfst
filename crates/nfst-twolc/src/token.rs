//! Tokens emitted by the twolc lexer. One enum collapses the upstream
//! pre1/pre2/pre3 token sets — the staging is an implementation detail
//! we discard.

use smol_str::SmolStr;

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    // ──────────────── section headers ────────────────
    SectionAlphabet,
    SectionDiacritics,
    SectionSets,
    SectionDefinitions,
    SectionRules,

    // ──────────────── keywords ────────────────
    Where,
    Except,
    Matched,
    Mixed,
    Freely,
    In,
    And,

    // ──────────────── unary operators ────────────────
    Star,            // *
    Plus,            // +
    FreelyInsert,    // /
    Complement,      // ~
    TermComplement,  // \
    Containment,     // $
    ContainmentOnce, // $.

    // ──────────────── binary operators ────────────────
    Union,        // |
    Intersection, // &
    Difference,   // -
    Power,        // ^

    // ──────────────── rule arrows ────────────────
    LeftArrow,              // <=
    RightArrow,             // =>
    LeftRightArrow,         // <=>
    LeftRestrictionArrow,   // /<=
    ReLeftArrow,            // <==
    ReRightArrow,           // ==>
    ReLeftRightArrow,       // <==>
    ReLeftRestrictionArrow, // /<==

    // ──────────────── brackets ────────────────
    LeftBracket,      // [
    RightBracket,     // ]
    LeftParenthesis,  // (
    RightParenthesis, // )
    LeftCurly,        // {
    RightCurly,       // }
    ReLeftBracket,    // <[
    ReRightBracket,   // ]>

    // ──────────────── structural ────────────────
    Colon,        // :
    Semicolon,    // ;
    Equals,       // =
    CenterMarker, // _
    QuestionMark, // ?
    Comma,        // ,

    // ──────────────── value-bearing ────────────────
    /// `"…"` — only at rule-start position.
    RuleName(SmolStr),
    /// FREE_SYMBOL+ (lexer's NAME_CH set after `%`-escape stripping).
    /// Digits are part of Symbol content; the parser converts to a count
    /// when it sees `^N` or `^N,K` repetition.
    Symbol(SmolStr),
}
