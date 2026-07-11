//! Parser and AST for the **xfst** command-shell language.
//!
//! xfst is the REPL/script surface that ties the rest of HFST together.
//! Each script is a flat sequence of commands operating on an implicit
//! transducer stack; embedded `regex …;` and `define NAME …;` forms
//! delegate their bodies to [`nfst_xre`].
//!
//! Parse-only: no transducer engine, no stack model, no command
//! execution.

mod ast;
mod display;
mod lexer;
mod parser;
mod token;

pub use ast::*;
pub use display::{pretty_print, strip_groups};
pub use lexer::{LexError, tokenize};
pub use parser::{ParseError, parse};
pub use token::{CommandKind, Token};

pub use nfst_syntax::{Diagnostic, Severity, Span, Spanned};
pub use nfst_xre::SpannedXre;
