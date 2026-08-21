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

impl Token {
    /// How the token is written in source, for diagnostics. Value-bearing
    /// tokens render their content, so a parse error names the symbol the
    /// reader can find on the offending line.
    pub fn as_source(&self) -> &str {
        match self {
            Token::SectionAlphabet => "Alphabet",
            Token::SectionDiacritics => "Diacritics",
            Token::SectionSets => "Sets",
            Token::SectionDefinitions => "Definitions",
            Token::SectionRules => "Rules",
            Token::Where => "where",
            Token::Except => "except",
            Token::Matched => "matched",
            Token::Mixed => "mixed",
            Token::Freely => "freely",
            Token::In => "in",
            Token::And => "and",
            Token::Star => "*",
            Token::Plus => "+",
            Token::FreelyInsert => "/",
            Token::Complement => "~",
            Token::TermComplement => "\\",
            Token::Containment => "$",
            Token::ContainmentOnce => "$.",
            Token::Union => "|",
            Token::Intersection => "&",
            Token::Difference => "-",
            Token::Power => "^",
            Token::LeftArrow => "<=",
            Token::RightArrow => "=>",
            Token::LeftRightArrow => "<=>",
            Token::LeftRestrictionArrow => "/<=",
            Token::ReLeftArrow => "<==",
            Token::ReRightArrow => "==>",
            Token::ReLeftRightArrow => "<==>",
            Token::ReLeftRestrictionArrow => "/<==",
            Token::LeftBracket => "[",
            Token::RightBracket => "]",
            Token::LeftParenthesis => "(",
            Token::RightParenthesis => ")",
            Token::LeftCurly => "{",
            Token::RightCurly => "}",
            Token::ReLeftBracket => "<[",
            Token::ReRightBracket => "]>",
            Token::Colon => ":",
            Token::Semicolon => ";",
            Token::Equals => "=",
            Token::CenterMarker => "_",
            Token::QuestionMark => "?",
            Token::Comma => ",",
            Token::RuleName(s) | Token::Symbol(s) => s.as_str(),
        }
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::RuleName(s) => write!(f, "rule name \"{s}\""),
            other => write!(f, "`{}`", other.as_source()),
        }
    }
}

/// Renders what the parser actually found, for a `expected …, got …`
/// diagnostic. `None` is the end of the token stream, which has no source
/// text to quote.
pub(crate) fn describe(token: Option<&Token>) -> String {
    match token {
        Some(t) => t.to_string(),
        None => "end of input".to_string(),
    }
}
