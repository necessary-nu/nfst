//! Tokens emitted by the pmatch lexer. Mirrors `pmatch_lex.ll`'s set,
//! collapsed to one enum.

use smol_str::SmolStr;

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    // ──────────────── statement keywords ────────────────
    Define,
    DefIns,
    Regex,
    SetVariable,
    DefinedList,

    // ──────────────── function-call keyword starts (consumed `(`) ────────────────
    LitLeft,
    InsLeft,
    EndTagLeft,
    CaptureLeft,
    CapLeft,
    OptCapLeft,
    ToLowerLeft,
    ToUpperLeft,
    OptToLowerLeft,
    OptToUpperLeft,
    AnyCaseLeft,
    ExplodeLeft,
    ImplodeLeft,
    LcLeft,
    RcLeft,
    NlcLeft,
    NrcLeft,
    OrLeft,
    AndLeft,
    TagLeft,
    WithLeft,
    LstLeft,
    ExcLeft,
    LikeLeft,
    UnlikeLeft,
    InterpolateLeft,
    SigmaLeft,
    CounterLeft,
    DefineLeft,
    UncomposeLeft,
    /// `userfunc(` — user-defined function call (the `(` is consumed).
    SymbolWithLeftParen(SmolStr),

    // ──────────────── acceptors ────────────────
    Alpha,
    UppercaseAlpha,
    LowercaseAlpha,
    Num,
    Punct,
    Whitespace,

    // ──────────────── variable names (only valid after `set`) ────────────────
    VariableName(SmolStr),

    // ──────────────── operators (xre suite) ────────────────
    Complement,
    TermComplement,
    Intersection,
    Minus,
    Plus,
    Star,
    Union,
    Containment,
    ContainmentOnce,
    ContainmentOpt,
    IgnoreInternally,
    Ignoring,
    Composition,
    LenientComposition,
    MergeRightArrow,
    MergeLeftArrow,
    CrossProduct,
    UpperPriorityUnion,
    LowerPriorityUnion,
    UpperMinus,
    LowerMinus,
    SubstituteLeft,

    LeftRestriction,
    LeftRightArrow,
    LeftArrow,
    RightArrow,
    ReplaceRight,
    OptionalReplaceRight,
    ReplaceLeft,
    OptionalReplaceLeft,
    ReplaceLeftRight,
    OptionalReplaceLeftRight,
    LtrLongestMatch,
    LtrShortestMatch,
    RtlLongestMatch,
    RtlShortestMatch,

    ReplaceContextUu,
    ReplaceContextLu,
    ReplaceContextUl,
    ReplaceContextLl,
    CenterMarker,
    MarkupMarker,
    LeftQuotient,

    Reverse,
    Invert,
    UpperProject,
    LowerProject,
    Shuffle,
    Before,
    After,

    Equals,
    EpsilonToken,
    AnyToken,
    BoundaryMarker,

    Comma,
    Commacomma,

    // ──────────────── PAIR_SEPARATOR variants ────────────────
    PairSeparator,
    /// `<ws>:<ws>` — pmatch's `?:?` shorthand.
    PairSeparatorSole,
    /// `<ws>:` — pmatch's `?:expr` shorthand (left side is `?`).
    PairSeparatorWoLeft,
    /// `:<ws>` — pmatch's `expr:?` shorthand (right side is `?`).
    PairSeparatorWoRight,

    // ──────────────── brackets ────────────────
    LeftBracket,
    RightBracket,
    LeftParenthesis,
    RightParenthesis,
    LeftBracketDotted,
    RightBracketDotted,

    // ──────────────── catenation N ────────────────
    CatenateN(u32),
    CatenateNPlus(u32),
    CatenateNMinus(u32),
    CatenateNToK(u32, u32),

    // ──────────────── value-bearing ────────────────
    Symbol(SmolStr),
    QuotedLiteral(SmolStr),
    CurlyLiteral(SmolStr),
    /// `"X-Y"` with single-codepoint X and Y.
    CharacterRange(SmolStr, SmolStr),
    Weight(f64),
    /// `;` or `; ::w` — pmatch's terminator carries an optional weight.
    EndOfWeightedExpression(f64),

    // ──────────────── @-prefixed file references (path stored only) ────────────────
    ReadBin(SmolStr),
    ReadText(SmolStr),
    ReadSpaced(SmolStr),
    ReadProlog(SmolStr),
    ReadLexc(SmolStr),
    ReadRe(SmolStr),
    ReadVec(SmolStr),
}
