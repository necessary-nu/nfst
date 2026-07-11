//! Parser and AST for the **twolc** two-level morphology language.
//!
//! Upstream's C++ implements twolc as a 3-stage text pipeline (pre1 →
//! pre2 → pre3) for historical Bison reasons. We collapse the staging to
//! a single recursive-descent parse pass; the resulting AST preserves
//! `where` clauses verbatim so the (future) evaluator can do variable
//! expansion.
//!
//! Parse-only: no transducer engine, no conflict resolution, no alphabet
//! completion.

mod ast;
mod display;
mod lexer;
mod parser;
mod token;

pub use ast::*;
pub use display::{pretty_print, strip_groups};
pub use lexer::{LexError, tokenize};
pub use parser::{ParseError, parse};
pub use token::Token;

pub use nfst_syntax::{Diagnostic, Severity, Span, Spanned};
