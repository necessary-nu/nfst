//! Tokens emitted by the xfst lexer.
//!
//! xfst is a command shell, so most tokens are dedicated command
//! keywords. Multi-word commands like `compose net` are recognised as
//! a single token: the lexer maps every accepted spelling to one
//! [`CommandKind`].

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Token {
    /// A command keyword. The lexer normalises every accepted spelling
    /// (including aliases like `compose`/`compose net`) to a single
    /// [`CommandKind`].
    Command(CommandKind),

    /// A free-form symbol (file path, name, number, etc.).
    Name(String),

    /// `regex …;` or `define NAME …;` body — the raw substring between
    /// the keyword and the closing `;`. Handed to `nfst_xre::parse`.
    RegexBody(String),

    /// `apply up/down/med <text> <ctrl-d>` body — the raw substring.
    ApplyBody(String),

    /// `read text/spaced-text <text> <ctrl-d>` body — the raw substring.
    HeredocBody(String),

    /// `>NAME` or `> NAME` — output redirect target.
    RedirectOut(String),

    /// `>>NAME` — append redirect target.
    RedirectAppend(String),

    /// `<NAME` — input redirect source.
    RedirectIn(String),

    /// `name-name` symbolic range used in `list`.
    Range(String, String),

    /// `(a, b, c)` — function prototype parameter list.
    Prototype(String),

    /// `,` — argument separator.
    Comma,
    /// `(` — left parenthesis.
    LeftParen,
    /// `)` — right parenthesis.
    RightParen,
    /// `[` — left bracket.
    LeftBracket,
    /// `]` — right bracket.
    RightBracket,
    /// `;` — command terminator.
    Semicolon,
    /// `:` — colon.
    Colon,
    /// `END` or `END;` — end-of-substitution marker.
    EndSub,
    /// Ctrl-D — heredoc terminator (raw, when not consumed inline).
    CtrlD,
}

/// One variant per distinct xfst command. Aliases (e.g. `compose` vs.
/// `compose net`) collapse to the same kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandKind {
    // ── regex / define ───────────────────────────────
    /// `regex` / `read regex` — body parsed via nfst-xre.
    ReadRegex,
    /// `define NAME` — name plus body parsed via nfst-xre.
    DefineName,
    /// `define NAME(args)` — function with parameters.
    DefineFunction,
    /// `alias` — define a command alias.
    DefineAlias,
    /// `undefine`
    Undefine,
    /// `unlist`
    Unlist,
    /// `list`
    List,

    // ── apply / lookup ───────────────────────────────
    /// `apply up` / `up` (heredoc form).
    ApplyUp,
    /// `apply up <text>` / `up <text>` (single-line form).
    ApplyUpSingle,
    ApplyDown,
    ApplyDownSingle,
    ApplyMed,
    LookupOptimize,
    RemoveOptimization,

    // ── stack ────────────────────────────────────────
    Clear,
    Pop,
    PushDefined,
    Turn,
    Rotate,
    Loads, // load / load stack
    Loadd, // load defined / loadd

    // ── network ops (unary) ──────────────────────────
    Invert,
    Reverse,
    Determinize,
    Minimize,
    EpsilonRemove,
    PruneNet,
    Negate,
    OnePlus,
    ZeroPlus,
    Sort,
    Shuffle,
    Substring,
    Cleanup,
    Complete,
    LowerSide,
    UpperSide,
    Sigma, // `sigma net` — distinct from `print sigma`
    LabelNet,
    Inspect,
    TwosidedFlags,
    EliminateFlag,
    EliminateAll,
    CollectEpsilonLoops,
    CompactSigma,
    Name,
    View,
    Hfst,
    ExtractAmbiguous,
    ExtractUnambiguous,
    Ambiguous,
    CompileReplaceLower,
    CompileReplaceUpper,

    // ── network ops (binary) ─────────────────────────
    Compose,
    Concatenate,
    Intersect,
    Union,
    Minus,
    Crossproduct,
    XfstIgnore,

    // ── print ────────────────────────────────────────
    Print, // `print net`
    PrintStack,
    PrintSigma,
    PrintSigmaCount,
    PrintSigmaWordCount,
    PrintSize,
    PrintLongestString,
    PrintLongestStringSize,
    PrintShortestString,
    PrintShortestStringSize,
    PrintFlags,
    PrintLabels,
    PrintLabelCount,
    PrintLabelmaps,
    PrintName,
    PrintAliases,
    PrintArccount,
    PrintDefined,
    PrintDir,
    PrintFileInfo,
    PrintList,
    PrintLists,
    PrintWords,
    PrintLowerWords,
    PrintUpperWords,
    PrintRandomWords,
    PrintRandomLower,
    PrintRandomUpper,
    PrintProps,

    // ── save / write ─────────────────────────────────
    SaveStack,
    SaveProlog,
    SaveSpaced,
    SaveText,
    SaveDot,
    SaveDefinition,
    SaveDefinitions,
    WriteAtt,

    // ── read ─────────────────────────────────────────
    ReadText,
    ReadSpaced,
    ReadProlog,
    ReadProps,
    ReadLexc,
    ReadAtt,

    // ── test ─────────────────────────────────────────
    TestEq,
    TestFunct,
    TestId,
    TestNull,
    TestNonnull,
    TestOverlap,
    TestSublanguage,
    TestUnambiguous,
    TestInfinitelyAmbiguous,
    TestLowerBounded,
    TestLowerUni,
    TestUpperBounded,
    TestUpperUni,

    // ── system / shell ───────────────────────────────
    Echo,
    Quit,
    System,
    Source,
    Apropos,
    Describe,
    Assert,

    // ── variables ────────────────────────────────────
    Set,
    Show,
    ShowAll,

    // ── substitute ───────────────────────────────────
    SubstituteNamed,
    SubstituteLabel,
    SubstituteSymbol,

    // ── property table ───────────────────────────────
    AddProps,
    EditProps,

    // ── misc structural ──────────────────────────────
    For,
}
