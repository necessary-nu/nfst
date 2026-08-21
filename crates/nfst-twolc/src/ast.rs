//! Typed AST for twolc source.
//!
//! Single-pass output. Sections appear in the order the upstream grammar
//! requires (`Alphabet`? `Diacritics`? `Sets`? `Definitions`? `Rules`),
//! with their own dedicated fields.
//!
//! `where` clauses are preserved as `VariableBlock`s on the parent rule;
//! variable expansion is the evaluator's job.

use nfst_syntax::Spanned;
pub use nfst_xre::{BinaryOp, UnaryOp};
use smol_str::SmolStr;

#[derive(Clone, Debug, PartialEq)]
pub struct TwolcFile {
    pub alphabet: Vec<Spanned<AlphabetPair>>,
    pub diacritics: Vec<Spanned<SmolStr>>,
    pub sets: Vec<Spanned<SetDefinition>>,
    pub definitions: Vec<Spanned<TwolcDefinition>>,
    pub rules: Vec<Spanned<TwolcRule>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AlphabetPair {
    pub upper: SmolStr,
    pub lower: SmolStr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SetDefinition {
    pub name: SmolStr,
    pub members: Vec<SmolStr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TwolcDefinition {
    pub name: SmolStr,
    pub body: Spanned<TwolcRegex>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TwolcRule {
    pub name: SmolStr,
    pub center: RuleCenter,
    pub operator: RuleOp,
    pub positive_contexts: Vec<RuleContext>,
    pub negative_contexts: Vec<RuleContext>,
    pub variables: Option<Vec<VariableBlock>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RuleCenter {
    /// `a:b` or `a:b | c:d` — flat list of alternatives.
    Pair(Vec<CenterPair>),
    /// `:[ E ]:` — regex-form rule center.
    Regex(Box<Spanned<TwolcRegex>>),
}

/// One alternative of a pair-form rule centre. Unlike an [`AlphabetPair`],
/// either side may be the `?` wildcard, so the sides cannot be plain strings —
/// the wildcard `?` and the escaped literal `%?` would both read as `"?"`.
#[derive(Clone, Debug, PartialEq)]
pub struct CenterPair {
    pub upper: CenterSide,
    pub lower: CenterSide,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CenterSide {
    /// `?` — any symbol, resolved by the consumer against the declared
    /// alphabet. Also what an elided side means: `a:` is `a:?`, `:b` is `?:b`.
    Any,
    /// A named symbol, `%`-escapes already resolved. An escaped `%?` lands
    /// here, as the literal one-character symbol `?`.
    Symbol(SmolStr),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RuleOp {
    /// `=>` — context restriction (right-arrow).
    Right,
    /// `<=` — left-arrow.
    Left,
    /// `<=>` — left-right arrow.
    LeftRight,
    /// `/<=` — left-restriction arrow.
    NotLeft,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuleContext {
    pub left: Spanned<TwolcRegex>,
    pub right: Spanned<TwolcRegex>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariableBlock {
    pub assignments: Vec<VariableAssignment>,
    pub matcher: VarMatcher,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariableAssignment {
    pub name: SmolStr,
    pub values: Vec<SmolStr>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VarMatcher {
    Matched,
    Mixed,
    Freely,
}

/// twolc's regex sublanguage. Smaller than xre — no replace arrows, no
/// `@`-files, no markup.
#[derive(Clone, Debug, PartialEq)]
pub enum TwolcRegex {
    Symbol(SmolStr),
    Pair {
        upper: Box<Spanned<TwolcRegex>>,
        lower: Box<Spanned<TwolcRegex>>,
    },
    Epsilon,
    Any,
    Group(Box<Spanned<TwolcRegex>>),
    Optional(Box<Spanned<TwolcRegex>>),
    Binary(BinaryOp, Box<Spanned<TwolcRegex>>, Box<Spanned<TwolcRegex>>),
    Unary(UnaryOp, Box<Spanned<TwolcRegex>>),
    /// `E ^ N`
    RepeatN(Box<Spanned<TwolcRegex>>, u32),
    /// `E ^ N,K`
    RepeatNToK(Box<Spanned<TwolcRegex>>, u32, u32),
}
