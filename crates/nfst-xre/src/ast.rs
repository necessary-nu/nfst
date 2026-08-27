//! Typed AST for xre source. Mirrors `xre_parse.yy`'s production tree, but
//! flattens the REGEXP1..REGEXP12 chain that exists only to encode operator
//! precedence — that information lives in the parser, not the tree.
//!
//! Every recursive child carries a `Span` via `nfst_syntax::Spanned<T>`. The
//! `Spanned` wrapper's equality ignores spans (see its docs), so AST snapshot
//! tests compare structure cleanly without becoming positional fixtures.

use nfst_syntax::Spanned;
use smol_str::SmolStr;

/// `Spanned<XreExpr>` — the canonical, public AST node type. Every recursive
/// position in the tree (including children of unary, binary, group, pair, …
/// nodes) is a boxed `SpannedXre`.
pub type SpannedXre = Spanned<XreExpr>;

#[derive(Clone, Debug, PartialEq)]
pub enum XreExpr {
    // ──────────────── atoms ────────────────
    /// A single label, e.g. `cat` or `+N`.
    Symbol(SmolStr),
    /// `{abc}` — curly form, expanded to a single-arc string at compile time.
    Curly(SmolStr),
    /// `0`, `""`, or `[]`.
    Epsilon,
    /// `?`
    Any,
    /// `.#.`
    BoundaryMarker,

    // ──────────────── label combinators ────────────────
    /// `upper:lower`. Either side can be a Symbol/Curly/Epsilon/Any/Boundary,
    /// or a fully-bracketed expression.
    Pair {
        upper: Box<SpannedXre>,
        lower: Box<SpannedXre>,
    },

    /// `LABEL::weight` or `[expr]::weight`.
    Weighted {
        expr: Box<SpannedXre>,
        weight: f64,
    },

    /// `@bin"..."`, `@txt"..."`, `@stxt"..."`, `@pl"..."`, `@re"..."`.
    ReadFile {
        kind: ReadKind,
        path: SmolStr,
    },

    /// `name(arg1, arg2, ...)` — `name` includes nothing; the trailing `(`
    /// captured by the lexer is dropped here.
    FunctionCall {
        name: SmolStr,
        args: Vec<SpannedXre>,
    },

    // ──────────────── grouping ────────────────
    /// `[ E ]` — explicit grouping (no semantic effect beyond precedence).
    Group(Box<SpannedXre>),
    /// `( E )` — makes the contained expression optional.
    Optional(Box<SpannedXre>),
    /// `[. E .]` — dotted-bracket grouping; `None` for the empty form `[..]`.
    BracketedDotted(Option<Box<SpannedXre>>),

    // ──────────────── operators ────────────────
    Unary(UnaryOp, Box<SpannedXre>),
    Binary(BinaryOp, Box<SpannedXre>, Box<SpannedXre>),

    /// `E^N`
    RepeatN(Box<SpannedXre>, u32),
    /// `E^>N`
    RepeatNPlus(Box<SpannedXre>, u32),
    /// `E^<N`
    RepeatNMinus(Box<SpannedXre>, u32),
    /// `E^N,K` or `E^{N,K}`
    RepeatNToK(Box<SpannedXre>, u32, u32),

    /// `$::w E` — containment with a weight.
    ContainmentWithWeight {
        expr: Box<SpannedXre>,
        weight: f64,
    },

    // ──────────────── replace and restriction ────────────────
    /// `mapping {,, mapping}* [|| ctx [, ctx]*]` etc.
    Replace {
        /// The first mapping's arrow, retained for callers that only ever deal
        /// in uniform rule lists. A compiler must read each `MappingPair`'s own
        /// `arrow` instead — a parallel list may mix them.
        arrow: ReplaceArrow,
        rules: Vec<ReplaceRule>,
    },

    /// `E => ctx [, ctx]*` — restriction rule.
    Restriction {
        body: Box<SpannedXre>,
        contexts: Vec<RestrContext>,
    },

    // ──────────────── substitute ────────────────
    /// `` `[ E, what, replacement... ] ``
    Substitute {
        haystack: Box<SpannedXre>,
        what: SubstituteWhat,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReadKind {
    /// `@bin"…"` or `@"…"`
    Binary,
    /// `@txt"…"`
    Text,
    /// `@stxt"…"`
    Spaced,
    /// `@pl"…"`
    Prolog,
    /// `@re"…"`
    Regex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Star,            // *
    Plus,            // +
    Reverse,         // .r
    Invert,          // .i
    UpperProject,    // .u
    LowerProject,    // .l
    Complement,      // ~
    TermComplement,  // \
    Containment,     // $
    ContainmentOnce, // $.
    ContainmentOpt,  // $?
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    /// implicit (juxtaposition)
    Concatenate,
    Compose,            // .o.
    LenientCompose,     // .O.
    CrossProduct,       // .x.
    MergeRight,         // .m>.
    MergeLeft,          // .<m.
    Before,             // <
    After,              // >
    Shuffle,            // <>
    Union,              // |
    Intersect,          // &
    Subtract,           // -
    UpperSubtract,      // .-u.
    LowerSubtract,      // .-l.
    UpperPriorityUnion, // .P.
    LowerPriorityUnion, // .p.
    Ignoring,           // /
    IgnoreInternally,   // ./.
    LeftQuotient,       // \\\
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReplaceArrow {
    Right,             // ->
    OptionalRight,     // (->)
    Left,              // <-
    OptionalLeft,      // (<-)
    LeftRight,         // <->
    OptionalLeftRight, // (<->)
    LtrLongest,        // @->
    LtrShortest,       // @>
    RtlLongest,        // ->@
    RtlShortest,       // >@
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContextMark {
    /// `||` — both sides match in upper alphabet.
    UpperUpper,
    /// `//` — context-on-lower, target-on-upper.
    LowerUpper,
    /// `\\` — context-on-upper, target-on-lower.
    UpperLower,
    /// `\/` — both sides on lower.
    LowerLower,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReplaceRule {
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
    pub left: Option<Box<SpannedXre>>,
    pub right: Option<Box<SpannedXre>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RestrContext {
    pub left: Option<Box<SpannedXre>>,
    pub right: Option<Box<SpannedXre>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MappingPair {
    pub upper: MappingSide,
    /// The arrow this mapping was written with. Upstream `regex.y` lexes an
    /// `ARROW` per `n0 ARROW n0` unit and `add_rule()` stores it per rule, so a
    /// parallel list may freely mix them: in `a -> b, c (->) d` the `c` mapping
    /// keeps its own optionality inside the shared context.
    pub arrow: ReplaceArrow,
    pub kind: MappingKind,
}

/// Distinguishes the two MAPPINGPAIR shapes in `xre_parse.yy`:
///
/// - `Plain`: `A -> B` — straightforward "replace A with B".
/// - `Markup`: `A -> B ... C` (or `A -> B ...`, or `A -> ... C`) — the
///   replacement is empty; `pre` and `post` form bracketing markup. Either
///   side of the markup may be absent (encoded as `None`).
///
/// This mirrors the four MAPPINGPAIR alternatives in the upstream grammar
/// faithfully: the C++ implementation always builds the markup variants as a
/// `(upper, empty)` mapping with a `(pre, post)` mark pair, and the absence
/// of pre/post is encoded as epsilon at compile time.
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
    /// Bare expression on this side.
    Expr(Box<SpannedXre>),
    /// `[. E .]` or `[..]` — dotted-bracket grouping in mapping position.
    /// `None` for the empty form `[..]`.
    Dotted(Option<Box<SpannedXre>>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SubstituteWhat {
    /// `` `[ E, sym, [list...] ] ``
    Symbol {
        needle: SmolStr,
        replacement: Vec<SmolStr>,
    },
    /// `` `[ E, a:b, c:d ] ``
    Pair {
        from: (SmolStr, SmolStr),
        to: (SmolStr, SmolStr),
    },
}
