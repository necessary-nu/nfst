//! Parser and AST for the Xerox regular-expression sublanguage (xre).
//!
//! Public surface:
//!
//! ```ignore
//! let tree = nfst_xre::parse("a b c ;")?;
//! ```
//!
//! This crate is parse-only: there is no transducer engine, no evaluator. The
//! deliverable is a typed `XreExpr` tree.

mod ast;
mod display;
mod lexer;
mod parser;
mod token;

pub use ast::*;
pub use display::{pretty_print, strip_groups};
pub use lexer::{LexError, tokenize};
pub use parser::{ParseError, parse, parse_all};
pub use token::Token;

// Re-export the diagnostic vocabulary from nfst-syntax so consumers don't have
// to depend on it directly for the common case of "parse, then look at the
// AST".
pub use nfst_syntax::{Diagnostic, Severity, Span, Spanned};
