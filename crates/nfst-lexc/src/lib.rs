//! Parser and AST for the **lexc** lexicon-compiler language.
//!
//! Embeds `nfst-xre` for regex bodies — both the `Definitions` section
//! (`Vowel = a | e | i ;`) and the `<xre> CONT ;` lexicon-entry shape pass
//! their bodies through `nfst_xre::parse` and store the resulting
//! `SpannedXre` directly in the lexc AST.
//!
//! Public surface:
//!
//! ```ignore
//! let file = nfst_lexc::parse(source)?;
//! for lexicon in &file.value.lexicons { … }
//! ```
//!
//! Parse-only: there is no transducer engine, no compilation pass.

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

// Re-export the diagnostic vocabulary so consumers don't need to depend on
// nfst-syntax directly for the common case.
pub use nfst_syntax::{Diagnostic, Severity, Span, Spanned};
