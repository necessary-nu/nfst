//! Typed AST for xfst source.
//!
//! Single-pass output. A script is a flat sequence of commands; each
//! command stores enough information for a future interpreter to
//! reconstruct what was asked. Embedded `regex E ;` and
//! `define NAME E ;` bodies are stored as parsed [`SpannedXre`] trees.

use nfst_syntax::Spanned;
use nfst_xre::SpannedXre;

#[derive(Clone, Debug, PartialEq)]
pub struct XfstScript {
    pub commands: Vec<Spanned<XfstCommand>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum XfstCommand {
    // ── regex / define ──────────────────────────────
    Regex(SpannedXre),
    Define {
        name: String,
        body: SpannedXre,
    },
    DefineFunction {
        name: String,
        params: Vec<String>,
        body: SpannedXre,
    },
    DefineAlias {
        name: String,
        body: String,
    },
    DefineList {
        name: String,
        members: Vec<String>,
    },
    Undefine(Vec<String>),
    Unlist(String),

    // ── stack ───────────────────────────────────────
    Clear,
    Pop,
    Push(String),
    Turn,
    Rotate,
    LoadStack(String),
    LoadDefinitions(String),

    // ── network ops ─────────────────────────────────
    Network(NetworkOp),

    // ── apply / lookup ──────────────────────────────
    Apply(ApplyKind, Option<String>),
    LookupOptimize,
    RemoveOptimization,

    // ── read / write / save ─────────────────────────
    Read(ReadCmd),
    Save(SaveCmd),

    // ── print ───────────────────────────────────────
    Print(PrintCmd),

    // ── test ────────────────────────────────────────
    Test(TestKind),

    // ── variables / show ────────────────────────────
    Set {
        var: String,
        value: String,
    },
    Show(Option<String>),
    Echo(String),
    System(String),
    Source(String),
    Quit,

    // ── substitute ──────────────────────────────────
    Substitute(SubstituteCmd),

    // ── help / misc ─────────────────────────────────
    Apropos(Option<String>),
    Describe(String),
    Assert(Box<Spanned<XfstCommand>>),
    AddProps(String),
    EditProps,
    Hfst(String),
    For,

    // ── i/o redirect wrapper ────────────────────────
    Redirected {
        command: Box<Spanned<XfstCommand>>,
        redirect: Redirect,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NetworkOp {
    Compose,
    Concatenate,
    Intersect,
    Union,
    Minus,
    Crossproduct,
    Ignore,
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
    Sigma,
    LabelNet,
    Inspect,
    TwosidedFlags,
    EliminateAll,
    CollectEpsilonLoops,
    CompactSigma,
    View,
    ExtractAmbiguous,
    ExtractUnambiguous,
    Ambiguous,
    CompileReplaceLower,
    CompileReplaceUpper,
    /// `eliminate flag NAME` — keeps a single argument.
    EliminateFlag(String),
    /// `name net NAME` / `name NAME`.
    Name(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PrintCmd {
    Net,
    Stack,
    Sigma,
    SigmaCount,
    SigmaWordCount,
    Size,
    LongestString,
    LongestStringSize,
    ShortestString,
    ShortestStringSize,
    Flags,
    Labels(Option<String>),
    LabelCount,
    LabelMaps,
    Name,
    Aliases,
    Arccount,
    Defined,
    Dir,
    FileInfo,
    List,
    Lists,
    Words(Option<u32>),
    LowerWords(Option<u32>),
    UpperWords(Option<u32>),
    RandomWords(Option<u32>),
    RandomLower(Option<u32>),
    RandomUpper(Option<u32>),
    Props,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ReadCmd {
    Text(String),
    Spaced(String),
    Prolog(String),
    Props(String),
    Lexc(String),
    Att(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SaveCmd {
    Stack(String),
    Prolog(String),
    Spaced(String),
    Text(String),
    Dot(String),
    Definition(String),
    Definitions(String),
    Att(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TestKind {
    Eq,
    Funct,
    Id,
    Null,
    Nonnull,
    Overlap,
    Sublanguage,
    Unambiguous,
    InfinitelyAmbiguous,
    LowerBounded,
    LowerUni,
    UpperBounded,
    UpperUni,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ApplyKind {
    Up,
    Down,
    Med,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SubstituteCmd {
    /// `substitute symbol N1 N2 ... for LABEL`
    Symbol {
        from: Vec<String>,
        to: String,
        scope: Option<String>,
    },
    /// `substitute label L1 L2 ... for LABEL`
    Label {
        from: Vec<String>,
        to: String,
        scope: Option<String>,
    },
    /// `substitute defined N for LABEL`
    Named { def: String, label: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Redirect {
    pub kind: RedirectKind,
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RedirectKind {
    In,
    Out,
    Append,
}
