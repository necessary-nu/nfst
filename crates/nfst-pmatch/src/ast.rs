//! Typed AST for pmatch source. Mirrors `pmatch_parse.yy`'s shape:
//! a top-level file is a sequence of statements (`Define`, `DefIns`,
//! `regex`, `set`, `list`, `@vec"…"`), each carrying a `PmatchExpr` body.
//!
//! The expression enum is large — pmatch is a superset of xre operator-
//! wise PLUS its own pattern-specific constructs (`EndTag`, `Capture`,
//! contexts, casing, like-arc, …). The xre operator enums (`BinaryOp`,
//! `UnaryOp`, `ReplaceArrow`, `ContextMark`, `ReadKind`) are reused so
//! the operator vocabulary stays canonical across the workspace.

use nfst_syntax::Spanned;
pub use nfst_xre::{BinaryOp, ContextMark, ReadKind, ReplaceArrow, UnaryOp};
use smol_str::SmolStr;

/// Shorthand for the canonical AST node type.
pub type SpannedExpr = Spanned<PmatchExpr>;

#[derive(Clone, Debug, PartialEq)]
pub struct PmatchFile {
    pub statements: Vec<Spanned<PmatchStatement>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PmatchStatement {
    /// `Define name expr ;` (or `define name expr ;`).
    /// `params` is `Some(_)` for function definitions like
    /// `Define name(a, b) expr ;`.
    Define {
        name: SmolStr,
        params: Option<Vec<SmolStr>>,
        body: SpannedExpr,
    },
    /// `DefIns name expr ;` — auto-Ins variant of Define.
    DefIns { name: SmolStr, body: SpannedExpr },
    /// `regex expr ;` — top-level expression named "TOP".
    RegexTop { body: SpannedExpr },
    /// `set var value` (no terminating `;`).
    SetVariable { name: SmolStr, value: VariableValue },
    /// `list name expr ;`
    ListDefinition { name: SmolStr, body: SpannedExpr },
    /// `@vec"path"` at statement position.
    ReadVec { path: SmolStr },
}

#[derive(Clone, Debug, PartialEq)]
pub enum VariableValue {
    /// A bare symbol (e.g. `off`, `on`, `1`, `cat`).
    Symbol(SmolStr),
    /// `0` / `""` / `[]` — epsilon.
    Epsilon,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PmatchExpr {
    // ──────────────── atoms ────────────────
    Symbol(SmolStr),
    /// `Lit(name)` shows up as a `PmatchString` in C++; we preserve the
    /// distinction.
    Literal(SmolStr),
    QuotedLiteral(SmolStr),
    CurlyLiteral(SmolStr),
    Epsilon,
    Any,
    BoundaryMarker,
    Acceptor(Acceptor),
    /// `"a-z"` or `"A-Z"` etc. — character range.
    CharacterRange {
        from: SmolStr,
        to: SmolStr,
    },

    // ──────────────── operators (reused from xre) ────────────────
    Binary(BinaryOp, Box<SpannedExpr>, Box<SpannedExpr>),
    Unary(UnaryOp, Box<SpannedExpr>),

    // ──────────────── grouping / weight / pair ────────────────
    Group(Box<SpannedExpr>),
    Optional(Box<SpannedExpr>),
    BracketedDotted(Option<Box<SpannedExpr>>),
    Pair {
        upper: Box<SpannedExpr>,
        lower: Box<SpannedExpr>,
    },
    Weighted {
        expr: Box<SpannedExpr>,
        weight: f64,
    },

    // ──────────────── catenation N ────────────────
    RepeatN(Box<SpannedExpr>, u32),
    RepeatNPlus(Box<SpannedExpr>, u32),
    RepeatNMinus(Box<SpannedExpr>, u32),
    RepeatNToK(Box<SpannedExpr>, u32, u32),

    // ──────────────── replacement and restriction ────────────────
    Replace {
        arrow: ReplaceArrow,
        rules: Vec<PmatchReplaceRule>,
    },
    Restriction {
        body: Box<SpannedExpr>,
        contexts: Vec<RestrContext>,
    },

    // ──────────────── pmatch-specific constructs ────────────────
    Ins(SmolStr),
    EndTag(SmolStr),
    Capture(SmolStr),
    Tag {
        body: Box<SpannedExpr>,
        name: SmolStr,
    },
    With {
        body: Box<SpannedExpr>,
        name: SmolStr,
        value: SmolStr,
    },
    Counter(SmolStr),
    CaseOp {
        op: CaseOp,
        side: Option<CaseSide>,
        body: Box<SpannedExpr>,
    },
    /// `Define( E )` wrapper at expression position (NOT the `Define name …`
    /// statement form — this is the same word reused as a function-call
    /// shape).
    DefineWrapper(Box<SpannedExpr>),
    Explode(Vec<SpannedExpr>),
    Implode(Vec<SpannedExpr>),
    Like {
        args: Vec<SmolStr>,
        threshold: Option<u32>,
        unlike: bool,
    },
    Lst(Box<SpannedExpr>),
    Exc(Box<SpannedExpr>),
    Sigma(Box<SpannedExpr>),
    Interpolate(Vec<SpannedExpr>),
    Substitute(Box<SpannedExpr>, Box<SpannedExpr>, Box<SpannedExpr>),
    Uncompose(Box<SpannedExpr>, Box<SpannedExpr>, Box<SpannedExpr>),

    // ──────────────── context conditions ────────────────
    Lc(Box<SpannedExpr>),
    Rc(Box<SpannedExpr>),
    Nlc(Box<SpannedExpr>),
    Nrc(Box<SpannedExpr>),
    OrContext(Vec<SpannedExpr>),
    AndContext(Vec<SpannedExpr>),

    // ──────────────── function call ────────────────
    Call {
        name: SmolStr,
        args: Vec<SpannedExpr>,
    },

    // ──────────────── file references (paths only) ────────────────
    ReadFile {
        kind: ReadKind,
        path: SmolStr,
    },
    ReadLexc(SmolStr),
    ReadVec(SmolStr),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Acceptor {
    Alpha,
    UppercaseAlpha,
    LowercaseAlpha,
    Num,
    Punct,
    Whitespace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CaseOp {
    Cap,
    OptCap,
    ToLower,
    ToUpper,
    OptToLower,
    OptToUpper,
    AnyCase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CaseSide {
    Upper,
    Lower,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PmatchReplaceRule {
    pub mappings: Vec<MappingPair>,
    pub contexts: Option<ReplaceContexts>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReplaceContexts {
    pub mark: ContextMark,
    pub items: Vec<ReplaceContext>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReplaceContext {
    pub left: Option<Box<SpannedExpr>>,
    pub right: Option<Box<SpannedExpr>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RestrContext {
    pub left: Option<Box<SpannedExpr>>,
    pub right: Option<Box<SpannedExpr>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MappingPair {
    pub upper: MappingSide,
    pub kind: MappingKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MappingKind {
    Plain {
        lower: MappingSide,
    },
    Markup {
        pre: Option<MappingSide>,
        post: Option<MappingSide>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum MappingSide {
    Expr(Box<SpannedExpr>),
    /// `[. E .]` or `[..]` in mapping position.
    Dotted(Option<Box<SpannedExpr>>),
}
