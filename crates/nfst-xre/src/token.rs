//! Tokens emitted by the xre lexer. Mirrors `xre_lex.ll`'s token set, with
//! payloads carried as Rust values (no `char *` lvalues / globals).
//!
//! Quoted/curly content has already had its outer quotes stripped by the
//! lexer; `%`-escapes are stripped from `Symbol` / `MultiCharSymbol` /
//! `FunctionName`. Quoted literals preserve their content as-is (the C++
//! distinguishes "single-character quoted" vs "multi-character quoted" but
//! that decision lives in the parser, not the AST).

use smol_str::SmolStr;

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    // ──────────────── single-character operators ────────────────
    Complement,      // ~
    TermComplement,  // \
    Intersection,    // &
    Minus,           // -
    Plus,            // +
    Star,            // *
    Union,           // |
    Before,          // <
    After,           // >
    Ignoring,        // /
    PairSeparator,   // :
    Comma,           // ,
    EndOfExpression, // ;
    AnyToken,        // ?

    // ──────────────── multi-character operators ────────────────
    Containment,        // $
    ContainmentOnce,    // $.
    ContainmentOpt,     // $?
    IgnoreInternally,   // ./.
    Shuffle,            // <>
    Composition,        // .o.
    LenientComposition, // .O.
    MergeRightArrow,    // .m>.
    MergeLeftArrow,     // .<m.
    CrossProduct,       // .x.
    UpperPriorityUnion, // .P.
    LowerPriorityUnion, // .p.
    UpperMinus,         // .-u.
    LowerMinus,         // .-l.
    SubstituteLeft,     // `

    LeftRestriction, // \<=
    LeftRightArrow,  // <=>
    LeftArrow,       // <=
    RightArrow,      // =>

    ReplaceRight,             // ->
    OptionalReplaceRight,     // (->)
    ReplaceLeft,              // <-
    OptionalReplaceLeft,      // (<-)
    ReplaceLeftRight,         // <->
    OptionalReplaceLeftRight, // (<->)
    LtrLongestMatch,          // @->
    LtrShortestMatch,         // @>
    RtlLongestMatch,          // ->@
    RtlShortestMatch,         // >@

    ReplaceContextUu, // ||
    ReplaceContextLu, // //
    ReplaceContextUl, // \\
    ReplaceContextLl, // \/

    CenterMarker, // _ (one or more)
    MarkupMarker, // ... (one or more times)
    LeftQuotient, // \\\

    Reverse,  // .r
    Invert,   // .i
    XreUpper, // .u
    XreLower, // .l

    Commacomma, // ,,

    // ──────────────── brackets ────────────────
    LeftBracket,        // [
    RightBracket,       // ]
    LeftParenthesis,    // (
    RightParenthesis,   // )
    LeftBracketDotted,  // [.
    RightBracketDotted, // .]

    // ──────────────── catenation N ────────────────
    CatenateN(u32),
    CatenateNPlus(u32),
    CatenateNMinus(u32),
    CatenateNToK(u32, u32),

    // ──────────────── value-bearing literals ────────────────
    Weight(f64),
    /// `"..."` body, outer quotes stripped.
    QuotedLiteral(SmolStr),
    /// `{...}` body, outer braces stripped.
    CurlyBrackets(SmolStr),

    /// `@bin"path"` or `@"path"`
    ReadBin(SmolStr),
    ReadText(SmolStr),
    ReadSpaced(SmolStr),
    ReadProlog(SmolStr),
    ReadRe(SmolStr),

    /// Single NAME_CH (or escape).
    Symbol(SmolStr),
    /// Two or more NAME_CH (or `0`-prefixed identifier, or `.#.`).
    MultiCharSymbol(SmolStr),
    /// `name(` — the trailing `(` belongs to the token in the upstream lexer.
    FunctionName(SmolStr),

    /// Empty alphabet: `0`, `""`, `[]`. Spelled the same in the AST.
    EpsilonToken,
}
