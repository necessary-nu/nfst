//! Parser and AST for the **pmatch** pattern-matching language.
//!
//! Pmatch is a superset of xre operator-wise: every xre operator is
//! reproduced natively. Where xre has a single tree-shaped expression
//! type, pmatch adds statement-level constructs (`Define`, `DefIns`,
//! `regex`, `set`, `list`) and a long list of pattern-specific operators
//! (`EndTag`, `Capture`, `LC`/`RC`/`NLC`/`NRC`, `Cap`, `OptCap`, `Like`,
//! `Counter`, `Tag`, `With`, `Sigma`, `Substitute`, `Uncompose`, …).
//!
//! Public surface:
//!
//! ```ignore
//! let file = nfst_pmatch::parse(source)?;
//! for st in &file.value.statements { … }
//! ```
//!
//! Parse-only: there is no transducer engine, no compilation, no
//! `@bin"…"`/`@vec"…"`/`@re"…"` resolution.

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

// Re-export the diagnostic vocabulary so consumers don't need a direct
// nfst-syntax dependency.
pub use nfst_syntax::{Diagnostic, Severity, Span, Spanned};
