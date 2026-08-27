//! Logos-driven xre lexer. One pass, byte-spanned tokens.
//!
//! The grammar of NAME_CH is delicate: `xre_lex.ll` defines an
//! `A7UNRESTRICTED` set as printable ASCII *minus* a list of operator
//! characters. We re-derive that set explicitly here rather than copy the
//! flex `{-}` set-difference syntax (logos's regex engine doesn't support
//! it). Allowed printable-ASCII NAME_CH characters are
//! `# ' ` =` plus `1-9`, `A-Z`, `a-z`. Plus any non-ASCII Unicode codepoint.
//! Plus `%X` (escape) where X is any character.

use crate::token::Token;
use logos::Logos;
use nfst_syntax::Span;
use smol_str::{SmolStr, SmolStrBuilder};

#[derive(Clone, Debug, Default, PartialEq)]
pub enum LexErrorKind {
    #[default]
    UnknownToken,
    BadWeight,
    BadInteger,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LexError {
    pub span: Span,
    pub kind: LexErrorKind,
    pub slice: SmolStr,
}

#[derive(Logos, Clone, Debug, PartialEq)]
#[logos(error = LexErrorKind)]
enum Raw {
    // Skip whitespace and `!`/`#` line comments. Modeled as variants with
    // `logos::skip` so we can give the comment regex an explicit priority,
    // which the `#[logos(skip ...)]` shorthand doesn't expose.
    #[regex(r"[ \t\r\n]+", logos::skip)]
    Whitespace,
    #[regex(r"[!#][^\n]*", logos::skip, priority = 5)]
    LineComment,

    // ────── multi-char operators (longest-match settles ties) ──────
    #[token("(<->)")]
    OptionalReplaceLeftRight,
    #[token("(->)")]
    OptionalReplaceRight,
    #[token("(<-)")]
    OptionalReplaceLeft,

    #[token(".m>.")]
    MergeRightArrow,
    #[token(".<m.")]
    MergeLeftArrow,
    #[token(".-u.")]
    UpperMinus,
    #[token(".-l.")]
    LowerMinus,

    #[token(".o.")]
    Composition,
    #[token(".O.")]
    LenientComposition,
    #[token(".x.")]
    CrossProduct,
    #[token(".P.")]
    UpperPriorityUnion,
    #[token(".p.")]
    LowerPriorityUnion,

    #[token("./.")]
    IgnoreInternally,

    #[token(".#.")]
    DotPoundDot, // becomes MultiCharSymbol(".#.")
    #[token("[.#.]")]
    BracketedPound, // becomes Symbol(".#.")

    /// `[.#.` (4 chars, no closing `]`). The upstream flex grammar matches
    /// this and unputs `.#.`, returning LEFT_BRACKET. We can't unput in
    /// logos, so this raw variant is later split into two `Token`s during
    /// `tokenize()`: `LeftBracket` (1 char) + `MultiCharSymbol(".#.")` (3
    /// chars). The `[.#.]` rule (5 chars) above takes precedence because of
    /// longest-match.
    #[token("[.#.")]
    LeftBracketPound,

    #[token(".r")]
    Reverse,
    #[token(".i")]
    Invert,
    #[token(".u")]
    XreUpper,
    #[token(".l")]
    XreLower,

    #[token("\\<=")]
    LeftRestriction,
    #[token("<=>")]
    LeftRightArrow,
    #[token("<->")]
    ReplaceLeftRight,
    #[token("<=")]
    LeftArrow,
    #[token("=>")]
    RightArrow,
    #[token("->@")]
    RtlLongestMatch,
    #[token("@->")]
    LtrLongestMatch,
    #[token("->")]
    ReplaceRight,
    #[token("<-")]
    ReplaceLeft,
    #[token(">@")]
    RtlShortestMatch,
    #[token("@>")]
    LtrShortestMatch,
    #[token("<>")]
    Shuffle,

    #[token("||")]
    ReplaceContextUu,
    #[token("//")]
    ReplaceContextLu,
    #[token("\\\\\\")]
    LeftQuotient,
    #[token("\\\\")]
    ReplaceContextUl,
    #[token("\\/")]
    ReplaceContextLl,

    #[regex(r"_+")]
    CenterMarker,
    #[regex(r"(\.\.\.)+")]
    MarkupMarker,

    #[token("$.")]
    ContainmentOnce,
    #[token("$?")]
    ContainmentOpt,
    #[token("$")]
    Containment,

    #[token(",,")]
    Commacomma,

    #[token("[.")]
    LeftBracketDotted,
    #[token(".]")]
    RightBracketDotted,
    #[token("[]")]
    EpsilonBrackets,

    // ────── single-char operators ──────
    #[token("~")]
    Complement,
    #[token("\\")]
    TermComplement,
    #[token("&")]
    Intersection,
    #[token("-")]
    Minus,
    #[token("+")]
    Plus,
    #[token("*")]
    Star,
    #[token("|")]
    Union,
    #[token("<")]
    Before,
    #[token(">")]
    After,
    #[token("/")]
    Ignoring,
    #[token(":")]
    PairSeparator,
    #[token(",")]
    Comma,
    #[token(";")]
    EndOfExpression,
    #[token("?")]
    AnyToken,
    // SubstituteLeft must outrank Symbol on a bare backtick (both have length 1).
    #[token("`", priority = 5)]
    SubstituteLeft,

    #[token("[")]
    LeftBracket,
    #[token("]")]
    RightBracket,
    #[token("(")]
    LeftParenthesis,
    #[token(")")]
    RightParenthesis,

    // ────── catenation N (^N, ^>N, ^<N, ^N,K, ^{N,K}) ──────
    #[regex(r"\^\{(?:[1-9][0-9]*|0),(?:[1-9][0-9]*|0)\}", catenate_n_to_k_braced)]
    #[regex(r"\^[ \t]*(?:[1-9][0-9]*|0),(?:[1-9][0-9]*|0)", catenate_n_to_k_bare)]
    CatenateNToK((u32, u32)),

    #[regex(r"\^>[ \t]*(?:[1-9][0-9]*|0)", catenate_n_plus)]
    CatenateNPlus(u32),

    #[regex(r"\^<[ \t]*(?:[1-9][0-9]*|0)", catenate_n_minus)]
    CatenateNMinus(u32),

    #[regex(r"\^[ \t]*(?:[1-9][0-9]*|0)", catenate_n)]
    CatenateN(u32),

    // ────── @-quoted file references ──────
    #[regex(r#"@bin"[^"]+""#, |lex| at_quoted(lex.slice(), "@bin\""))]
    #[regex(r#"@"[^"]+""#, |lex| at_quoted(lex.slice(), "@\""))]
    ReadBin(SmolStr),

    #[regex(r#"@txt"[^"]+""#, |lex| at_quoted(lex.slice(), "@txt\""))]
    ReadText(SmolStr),

    #[regex(r#"@stxt"[^"]+""#, |lex| at_quoted(lex.slice(), "@stxt\""))]
    ReadSpaced(SmolStr),

    #[regex(r#"@pl"[^"]+""#, |lex| at_quoted(lex.slice(), "@pl\""))]
    ReadProlog(SmolStr),

    #[regex(r#"@re"[^"]+""#, |lex| at_quoted(lex.slice(), "@re\""))]
    ReadRe(SmolStr),

    // ────── weight ────── (`::` followed by signed decimal)
    #[regex(r"::-?[0-9]+(\.[0-9]+)?", weight)]
    Weight(f64),

    // ────── empty epsilon ──────
    #[token("\"\"")]
    EpsilonQuotes,

    // ────── quoted literal "..." (non-empty body) ──────
    #[regex(r#""[^"]+""#, quoted_literal)]
    QuotedLiteral(SmolStr),

    // ────── curly brackets {...} ──────
    #[regex(r"\{[^}]+\}", curly_body)]
    CurlyBrackets(SmolStr),

    // ────── 0 (epsilon when standalone, multichar when followed by name chars) ──────
    // Order matters: the multichar+open-paren rules (FunctionName) come first
    // because they're longer.
    #[regex(r"0(?:%.|[^\x00-\x7f]|[#'`=1-9A-Za-z]|0)+\(", function_name_owned)]
    #[regex(
        r"(?:%.|[^\x00-\x7f]|['`=1-9A-Za-z])(?:%.|[^\x00-\x7f]|[#'`=1-9A-Za-z]|0)*\(",
        function_name_owned
    )]
    // Builtin function keywords (regex.l has one hardcoded rule per name).
    // These start with `_`, which is deliberately not a NAME_CH, so they can
    // never be reached by the generic FunctionName patterns above. Logos
    // resolves overlaps by longest match, so `_eq(` wins over CenterMarker's
    // `_+` without depending on declaration order.
    #[regex(
        r"_(?:S|isunambiguous|isidentity|isfunctional|notid|lm|loweruniqeps|loweruniq|allfinal|unambpart|ambpart|ambdom|eq|marktail|addfinalloop|addnonfinalloop|addloop|addsink|leftrewr|flatten|sublabel|closeu|close)\(",
        function_name_owned
    )]
    FunctionName(SmolStr),

    #[regex(r"0(?:%.|[^\x00-\x7f]|[#'`=1-9A-Za-z]|0)+", strip_percents_owned)]
    MultiCharFromZero(SmolStr),

    // ────── identifier / multichar symbol / single symbol ──────
    // Single NAME_CH is SYMBOL; longer forms collapse to MULTICHAR_SYMBOL.
    // `#` is intentionally NOT a valid first NAME_CH — upstream flex resolves
    // ties in favor of the comment rule, which always starts with `!` or `#`.
    // Continuation positions still allow `#` so that `abc#def` is one
    // multichar symbol.
    #[regex(
        r"(?:%.|[^\x00-\x7f]|['`=1-9A-Za-z])(?:%.|[^\x00-\x7f]|[#'`=1-9A-Za-z]|0)+",
        strip_percents_owned
    )]
    MultiCharSymbol(SmolStr),

    #[regex(r"%.|[^\x00-\x7f]|['`=1-9A-Za-z]", strip_percents_owned)]
    Symbol(SmolStr),

    #[token("0")]
    EpsilonZero,
}

fn at_quoted(slice: &str, prefix: &str) -> SmolStr {
    let inner = slice.strip_prefix(prefix).unwrap_or(slice);
    let inner = inner.strip_suffix('"').unwrap_or(inner);
    inner.into()
}

fn weight(lex: &mut logos::Lexer<Raw>) -> Result<f64, LexErrorKind> {
    let s = lex.slice();
    s.strip_prefix("::")
        .and_then(|n| n.parse::<f64>().ok())
        .ok_or(LexErrorKind::BadWeight)
}

fn quoted_literal(lex: &mut logos::Lexer<Raw>) -> SmolStr {
    let s = lex.slice();
    s.trim_matches('"').into()
}

fn curly_body(lex: &mut logos::Lexer<Raw>) -> SmolStr {
    let s = lex.slice();
    s.strip_prefix('{')
        .and_then(|t| t.strip_suffix('}'))
        .unwrap_or(s)
        .into()
}

fn function_name_owned(lex: &mut logos::Lexer<Raw>) -> SmolStr {
    // Includes trailing `(` to mirror the C++ FUNCTION_NAME shape.
    strip_percents(lex.slice())
}

fn strip_percents_owned(lex: &mut logos::Lexer<Raw>) -> SmolStr {
    strip_percents(lex.slice())
}

/// Remove `%` escape characters: each `%` is dropped, the next codepoint
/// kept verbatim. Mirrors `hfst::xre::strip_percents`.
fn strip_percents(s: &str) -> SmolStr {
    let mut out = SmolStrBuilder::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            if let Some(escaped) = chars.next() {
                out.push(escaped);
            }
        } else {
            out.push(c);
        }
    }
    out.finish()
}

fn parse_pair(s: &str) -> Result<(u32, u32), LexErrorKind> {
    let mut parts = s.split(',');
    let lhs = parts.next().ok_or(LexErrorKind::BadInteger)?;
    let rhs = parts.next().ok_or(LexErrorKind::BadInteger)?;
    let n = lhs.parse::<u32>().map_err(|_| LexErrorKind::BadInteger)?;
    let k = rhs.parse::<u32>().map_err(|_| LexErrorKind::BadInteger)?;
    Ok((n, k))
}

fn catenate_n_to_k_braced(lex: &mut logos::Lexer<Raw>) -> Result<(u32, u32), LexErrorKind> {
    let s = lex.slice();
    let inner = s
        .strip_prefix("^{")
        .and_then(|t| t.strip_suffix('}'))
        .ok_or(LexErrorKind::BadInteger)?;
    parse_pair(inner)
}

fn catenate_n_to_k_bare(lex: &mut logos::Lexer<Raw>) -> Result<(u32, u32), LexErrorKind> {
    let s = lex.slice();
    let body = s.strip_prefix('^').ok_or(LexErrorKind::BadInteger)?;
    parse_pair(body.trim_start())
}

fn catenate_n(lex: &mut logos::Lexer<Raw>) -> Result<u32, LexErrorKind> {
    let s = lex.slice();
    let body = s.strip_prefix('^').ok_or(LexErrorKind::BadInteger)?;
    body.trim_start()
        .parse::<u32>()
        .map_err(|_| LexErrorKind::BadInteger)
}

fn catenate_n_plus(lex: &mut logos::Lexer<Raw>) -> Result<u32, LexErrorKind> {
    let s = lex.slice();
    let body = s.strip_prefix("^>").ok_or(LexErrorKind::BadInteger)?;
    body.trim_start()
        .parse::<u32>()
        .map_err(|_| LexErrorKind::BadInteger)
}

fn catenate_n_minus(lex: &mut logos::Lexer<Raw>) -> Result<u32, LexErrorKind> {
    let s = lex.slice();
    let body = s.strip_prefix("^<").ok_or(LexErrorKind::BadInteger)?;
    body.trim_start()
        .parse::<u32>()
        .map_err(|_| LexErrorKind::BadInteger)
}

/// Translate the internal `Raw` token into the public `Token`. Most are 1:1;
/// the special cases collapse `Raw::DotPoundDot`/`BracketedPound`/etc into
/// the canonical multichar/single shapes the parser expects.
fn lift(raw: Raw) -> Token {
    match raw {
        Raw::OptionalReplaceLeftRight => Token::OptionalReplaceLeftRight,
        Raw::OptionalReplaceRight => Token::OptionalReplaceRight,
        Raw::OptionalReplaceLeft => Token::OptionalReplaceLeft,
        Raw::MergeRightArrow => Token::MergeRightArrow,
        Raw::MergeLeftArrow => Token::MergeLeftArrow,
        Raw::UpperMinus => Token::UpperMinus,
        Raw::LowerMinus => Token::LowerMinus,
        Raw::Composition => Token::Composition,
        Raw::LenientComposition => Token::LenientComposition,
        Raw::CrossProduct => Token::CrossProduct,
        Raw::UpperPriorityUnion => Token::UpperPriorityUnion,
        Raw::LowerPriorityUnion => Token::LowerPriorityUnion,
        Raw::IgnoreInternally => Token::IgnoreInternally,
        Raw::DotPoundDot => Token::MultiCharSymbol(".#.".into()),
        Raw::BracketedPound => Token::Symbol(".#.".into()),
        Raw::Reverse => Token::Reverse,
        Raw::Invert => Token::Invert,
        Raw::XreUpper => Token::XreUpper,
        Raw::XreLower => Token::XreLower,
        Raw::LeftRestriction => Token::LeftRestriction,
        Raw::LeftRightArrow => Token::LeftRightArrow,
        Raw::ReplaceLeftRight => Token::ReplaceLeftRight,
        Raw::LeftArrow => Token::LeftArrow,
        Raw::RightArrow => Token::RightArrow,
        Raw::RtlLongestMatch => Token::RtlLongestMatch,
        Raw::LtrLongestMatch => Token::LtrLongestMatch,
        Raw::ReplaceRight => Token::ReplaceRight,
        Raw::ReplaceLeft => Token::ReplaceLeft,
        Raw::RtlShortestMatch => Token::RtlShortestMatch,
        Raw::LtrShortestMatch => Token::LtrShortestMatch,
        Raw::Shuffle => Token::Shuffle,
        Raw::ReplaceContextUu => Token::ReplaceContextUu,
        Raw::ReplaceContextLu => Token::ReplaceContextLu,
        Raw::LeftQuotient => Token::LeftQuotient,
        Raw::ReplaceContextUl => Token::ReplaceContextUl,
        Raw::ReplaceContextLl => Token::ReplaceContextLl,
        Raw::CenterMarker => Token::CenterMarker,
        Raw::MarkupMarker => Token::MarkupMarker,
        Raw::ContainmentOnce => Token::ContainmentOnce,
        Raw::ContainmentOpt => Token::ContainmentOpt,
        Raw::Containment => Token::Containment,
        Raw::Commacomma => Token::Commacomma,
        Raw::LeftBracketDotted => Token::LeftBracketDotted,
        Raw::RightBracketDotted => Token::RightBracketDotted,
        Raw::EpsilonBrackets => Token::EpsilonToken,
        Raw::EpsilonQuotes => Token::EpsilonToken,
        Raw::EpsilonZero => Token::EpsilonToken,
        Raw::Complement => Token::Complement,
        Raw::TermComplement => Token::TermComplement,
        Raw::Intersection => Token::Intersection,
        Raw::Minus => Token::Minus,
        Raw::Plus => Token::Plus,
        Raw::Star => Token::Star,
        Raw::Union => Token::Union,
        Raw::Before => Token::Before,
        Raw::After => Token::After,
        Raw::Ignoring => Token::Ignoring,
        Raw::PairSeparator => Token::PairSeparator,
        Raw::Comma => Token::Comma,
        Raw::EndOfExpression => Token::EndOfExpression,
        Raw::AnyToken => Token::AnyToken,
        Raw::SubstituteLeft => Token::SubstituteLeft,
        Raw::LeftBracket => Token::LeftBracket,
        Raw::RightBracket => Token::RightBracket,
        Raw::LeftParenthesis => Token::LeftParenthesis,
        Raw::RightParenthesis => Token::RightParenthesis,
        Raw::CatenateNToK((n, k)) => Token::CatenateNToK(n, k),
        Raw::CatenateNPlus(n) => Token::CatenateNPlus(n),
        Raw::CatenateNMinus(n) => Token::CatenateNMinus(n),
        Raw::CatenateN(n) => Token::CatenateN(n),
        Raw::ReadBin(p) => Token::ReadBin(p),
        Raw::ReadText(p) => Token::ReadText(p),
        Raw::ReadSpaced(p) => Token::ReadSpaced(p),
        Raw::ReadProlog(p) => Token::ReadProlog(p),
        Raw::ReadRe(p) => Token::ReadRe(p),
        Raw::Weight(w) => Token::Weight(w),
        Raw::QuotedLiteral(s) => Token::QuotedLiteral(s),
        Raw::CurlyBrackets(s) => Token::CurlyBrackets(s),
        Raw::FunctionName(s) => Token::FunctionName(s),
        Raw::MultiCharFromZero(s) => Token::MultiCharSymbol(s),
        Raw::MultiCharSymbol(s) => Token::MultiCharSymbol(s),
        Raw::Symbol(s) => Token::Symbol(s),
        Raw::LeftBracketPound | Raw::Whitespace | Raw::LineComment => {
            unreachable!("handled in tokenize() / logos::skip — never yielded here")
        }
    }
}

pub fn tokenize(source: &str) -> Result<Vec<(Token, Span)>, Vec<LexError>> {
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut lex = Raw::lexer(source);
    while let Some(item) = lex.next() {
        let span = Span::anonymous(lex.span());
        match item {
            // `[.#.` matched 4 chars but the upstream grammar treats those as
            // `[` (LeftBracket) followed by `.#.` (MultiCharSymbol). Re-emit
            // as two Tokens with subsplit spans.
            Ok(Raw::LeftBracketPound) => {
                let start = span.start();
                let lb_span = Span::anonymous(start..start + 1);
                let pound_span = Span::anonymous(start + 1..start + 4);
                tokens.push((Token::LeftBracket, lb_span));
                tokens.push((Token::MultiCharSymbol(".#.".into()), pound_span));
            }
            Ok(raw) => tokens.push((lift(raw), span)),
            Err(kind) => errors.push(LexError {
                span,
                kind,
                slice: lex.slice().into(),
            }),
        }
    }
    if errors.is_empty() {
        Ok(tokens)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(src: &str) -> Vec<Token> {
        tokenize(src)
            .expect("clean lex")
            .into_iter()
            .map(|(t, _)| t)
            .collect()
    }

    #[test]
    fn cats_and_dogs_basic() {
        let toks = lex("c a t");
        assert_eq!(
            toks,
            vec![
                Token::Symbol("c".into()),
                Token::Symbol("a".into()),
                Token::Symbol("t".into()),
            ]
        );
    }

    #[test]
    fn cats_and_dogs_with_pairs() {
        let toks = lex("c:d a:o t:g");
        use Token::*;
        assert_eq!(
            toks,
            vec![
                Symbol("c".into()),
                PairSeparator,
                Symbol("d".into()),
                Symbol("a".into()),
                PairSeparator,
                Symbol("o".into()),
                Symbol("t".into()),
                PairSeparator,
                Symbol("g".into()),
            ]
        );
    }

    #[test]
    fn weights() {
        assert_eq!(
            lex("a::1.5"),
            vec![Token::Symbol("a".into()), Token::Weight(1.5)]
        );
        assert_eq!(lex("::-3"), vec![Token::Weight(-3.0)]);
    }

    #[test]
    fn epsilon_aliases() {
        assert_eq!(
            lex(r#"0 "" []"#),
            vec![
                Token::EpsilonToken,
                Token::EpsilonToken,
                Token::EpsilonToken,
            ]
        );
    }

    #[test]
    fn replace_arrows() {
        use Token::*;
        let toks = lex("-> <- <-> (->) (<-) (<->) @-> ->@ @> >@");
        assert_eq!(
            toks,
            vec![
                ReplaceRight,
                ReplaceLeft,
                ReplaceLeftRight,
                OptionalReplaceRight,
                OptionalReplaceLeft,
                OptionalReplaceLeftRight,
                LtrLongestMatch,
                RtlLongestMatch,
                LtrShortestMatch,
                RtlShortestMatch,
            ]
        );
    }

    #[test]
    fn comments_skipped_both_styles() {
        // `foo` and `bar` are 3 NAME_CHs each, so they tokenize as MultiCharSymbol
        // (the C++ flex grammar maps single NAME_CH → SYMBOL and 2+ → MULTICHAR_SYMBOL).
        let toks = lex("! a comment\nfoo # another comment\nbar");
        assert_eq!(
            toks,
            vec![
                Token::MultiCharSymbol("foo".into()),
                Token::MultiCharSymbol("bar".into()),
            ]
        );
    }

    #[test]
    fn percent_escapes_are_stripped() {
        // %+N escapes the +; the resulting symbol is "+N"
        assert_eq!(lex("%+N"), vec![Token::MultiCharSymbol("+N".into())]);
    }

    #[test]
    fn quoted_literal_strips_quotes() {
        assert_eq!(lex(r#""foo""#), vec![Token::QuotedLiteral("foo".into())]);
    }

    #[test]
    fn curly_strips_braces() {
        assert_eq!(lex("{abc}"), vec![Token::CurlyBrackets("abc".into())]);
    }

    #[test]
    fn at_quoted_paths() {
        use Token::*;
        let toks = lex(r#"@"cat.foma" @bin"x.bin" @txt"a.txt" @stxt"a.stxt" @pl"a.pl" @re"a.re""#);
        assert_eq!(
            toks,
            vec![
                ReadBin("cat.foma".into()),
                ReadBin("x.bin".into()),
                ReadText("a.txt".into()),
                ReadSpaced("a.stxt".into()),
                ReadProlog("a.pl".into()),
                ReadRe("a.re".into()),
            ]
        );
    }

    #[test]
    fn catenation_n_variants() {
        use Token::*;
        let toks = lex("^3 ^>3 ^<3 ^3,5 ^{3,5}");
        assert_eq!(
            toks,
            vec![
                CatenateN(3),
                CatenateNPlus(3),
                CatenateNMinus(3),
                CatenateNToK(3, 5),
                CatenateNToK(3, 5),
            ]
        );
    }

    #[test]
    fn boundary_marker_forms() {
        // [.#.] is one Symbol(".#."), .#. on its own is MultiCharSymbol(".#.")
        assert_eq!(lex("[.#.]"), vec![Token::Symbol(".#.".into())]);
        assert_eq!(lex(".#."), vec![Token::MultiCharSymbol(".#.".into())]);
    }

    #[test]
    fn left_bracket_pound_unput_emulation() {
        // `[.#.foo]`: upstream flex matches `[.#.` (4 chars), pushes back
        // `.#.`, returning LEFT_BRACKET. We emit the same tokens.
        let toks = lex("[.#.foo]");
        assert_eq!(
            toks,
            vec![
                Token::LeftBracket,
                Token::MultiCharSymbol(".#.".into()),
                Token::MultiCharSymbol("foo".into()),
                Token::RightBracket,
            ]
        );
    }

    #[test]
    fn left_bracket_pound_subsplit_spans() {
        // The synthesized [` `] LeftBracket should span 1 byte, the
        // MultiCharSymbol `.#.` should span 3.
        let toks = tokenize("[.#.foo]").expect("clean lex");
        assert_eq!(toks[0].1.start(), 0);
        assert_eq!(toks[0].1.end(), 1);
        assert_eq!(toks[1].1.start(), 1);
        assert_eq!(toks[1].1.end(), 4);
    }

    #[test]
    fn function_name_includes_paren() {
        assert_eq!(lex("Concat("), vec![Token::FunctionName("Concat(".into())]);
    }

    #[test]
    fn longest_match_for_replace_arrows() {
        use Token::*;
        // (<->) must be one token even though <-> is also a token.
        assert_eq!(lex("(<->)"), vec![OptionalReplaceLeftRight]);
        assert_eq!(lex("<->"), vec![ReplaceLeftRight]);
    }

    #[test]
    fn unknown_token_errors() {
        // Bare backslash followed by unrecognized text is fine — `\` is TermComplement.
        // But truly unknown bytes (like `^!` with `!` being a comment...) — let's pick
        // something genuinely bad. The C++ lexer emits LEXER_ERROR for `.` (dot) alone.
        let r = tokenize(".");
        assert!(r.is_err(), "bare dot should be a lex error: got {r:?}");
    }
}
