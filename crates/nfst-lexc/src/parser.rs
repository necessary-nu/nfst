//! Recursive-descent parser for lexc. Each section is consumed by a
//! dedicated method; the lexer is responsible for state transitions between
//! sections, so the parser just dispatches on the next token kind.
//!
//! Definition bodies and `<xre>` lexicon entries are handed straight to
//! `nfst_xre::parse`, and the resulting `SpannedXre` is stored in the lexc
//! AST. xre parse errors bubble up as lexc `Diagnostic`s with their spans
//! shifted into the lexc source.

use crate::ast::{
    Definition, EntrySpec, LexcFile, Lexicon, LexiconEntry, LexiconName, MulticharSymbol,
};
use crate::lexer::{LexError, tokenize};
use crate::token::Token;
use nfst_syntax::{Diagnostic, Span, Spanned};
use smol_str::SmolStr;

#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse(source: &str) -> Result<Spanned<LexcFile>, ParseError> {
    let tokens = tokenize(source).map_err(|errs| ParseError {
        diagnostics: errs.into_iter().map(lex_error_to_diag).collect(),
    })?;
    let mut p = Parser::new(tokens);
    p.parse_file().map_err(|d| ParseError {
        diagnostics: vec![d],
    })
}

fn lex_error_to_diag(e: LexError) -> Diagnostic {
    Diagnostic::error(e.span, e.message)
}

// ───────────────────────── parser core ─────────────────────────

struct Parser {
    tokens: Vec<(Token, Span)>,
    pos: usize,
    last_end: usize,
}

impl Parser {
    fn new(tokens: Vec<(Token, Span)>) -> Self {
        Self {
            tokens,
            pos: 0,
            last_end: 0,
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|(t, _)| t)
    }

    fn peek_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|(_, s)| s.clone())
            .unwrap_or_else(|| Span::anonymous(self.last_end..self.last_end))
    }

    fn current_start(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map(|(_, s)| s.start())
            .unwrap_or(self.last_end)
    }

    fn merge(&self, start: usize) -> Span {
        Span::anonymous(start..self.last_end)
    }

    fn bump(&mut self) -> Option<(Token, Span)> {
        let item = self.tokens.get(self.pos).cloned();
        if let Some((_, ref s)) = item {
            self.last_end = s.end();
            self.pos += 1;
        }
        item
    }

    fn err(&self, msg: impl Into<String>) -> Diagnostic {
        Diagnostic::error(self.peek_span(), msg)
    }

    // ───────────────────────── top level ─────────────────────────

    fn parse_file(&mut self) -> Result<Spanned<LexcFile>, Diagnostic> {
        let start = self.current_start();
        let mut multichars = Vec::new();
        let mut noflags = Vec::new();
        let mut definitions = Vec::new();
        let mut lexicons = Vec::new();
        let mut has_end = false;

        while !self.is_at_end() {
            match self.peek() {
                Some(Token::SectionMulticharsStart { .. }) => {
                    self.bump();
                    self.parse_multichar_list(&mut multichars);
                }
                Some(Token::SectionNoFlagsStart) => {
                    self.bump();
                    self.parse_noflags_list(&mut noflags)?;
                }
                Some(Token::SectionDefinitionsStart) => {
                    self.bump();
                    self.parse_definition_list(&mut definitions)?;
                }
                Some(Token::LexiconStart { .. }) => {
                    let lex = self.parse_lexicon()?;
                    lexicons.push(lex);
                }
                Some(Token::EndKeyword) => {
                    self.bump();
                    has_end = true;
                    break;
                }
                _ => {
                    return Err(self.err(format!(
                        "expected a section header (Multichar_Symbols, NOFLAGS, \
                         Definitions, LEXICON, END), got {:?}",
                        self.peek()
                    )));
                }
            }
        }

        // Mirror the upstream `LEXC_FILE` production: `LEXICON_PART` is
        // mandatory (≥ 1 lexicon). Empty files and "headers only" files
        // are rejected here so consumers don't silently accept partial
        // inputs.
        if lexicons.is_empty() {
            return Err(Diagnostic::error(
                Span::anonymous(0..0),
                "lexc file must contain at least one LEXICON block",
            ));
        }

        let span = self.merge(start);
        Ok(Spanned::new(
            LexcFile {
                multichars,
                noflags,
                definitions,
                lexicons,
                has_end,
            },
            span,
        ))
    }

    // ───────────────────────── sections ─────────────────────────

    fn parse_multichar_list(&mut self, out: &mut Vec<Spanned<MulticharSymbol>>) {
        while let Some(Token::Identifier(_)) = self.peek() {
            let (tok, span) = self.bump().unwrap();
            if let Token::Identifier(s) = tok {
                out.push(Spanned::new(MulticharSymbol(s), span));
            }
        }
    }

    fn parse_noflags_list(
        &mut self,
        out: &mut Vec<Spanned<LexiconName>>,
    ) -> Result<(), Diagnostic> {
        loop {
            match self.peek() {
                Some(Token::Identifier(_)) => {
                    let (tok, span) = self.bump().unwrap();
                    if let Token::Identifier(s) = tok {
                        out.push(Spanned::new(LexiconName(s), span));
                    }
                }
                Some(Token::Semicolon) => {
                    self.bump();
                    return Ok(());
                }
                _ => {
                    // Section ended without `;` — the lexer should have
                    // flagged this, but recover gracefully.
                    return Ok(());
                }
            }
        }
    }

    fn parse_definition_list(
        &mut self,
        out: &mut Vec<Spanned<Definition>>,
    ) -> Result<(), Diagnostic> {
        while let Some(Token::Identifier(_)) = self.peek() {
            // Lookahead: only continue if the identifier is followed by `=`.
            let next_is_equals = matches!(
                self.tokens.get(self.pos + 1).map(|(t, _)| t),
                Some(Token::Equals)
            );
            if !next_is_equals {
                break;
            }
            out.push(self.parse_definition_line()?);
        }
        Ok(())
    }

    fn parse_definition_line(&mut self) -> Result<Spanned<Definition>, Diagnostic> {
        let start = self.current_start();
        let (name_tok, _) = self.bump().unwrap();
        let name = match name_tok {
            Token::Identifier(s) => s,
            _ => return Err(self.err("expected definition name")),
        };
        // consume `=`
        match self.peek() {
            Some(Token::Equals) => {
                self.bump();
            }
            _ => return Err(self.err("expected `=` after definition name")),
        }
        // body
        let (body_tok, body_span) = self
            .bump()
            .ok_or_else(|| self.err("expected definition body"))?;
        let body_str = match body_tok {
            Token::DefinitionBody(s) => s,
            other => {
                return Err(Diagnostic::error(
                    body_span,
                    format!("expected definition body, got {other:?}"),
                ));
            }
        };
        // semicolon
        match self.peek() {
            Some(Token::Semicolon) => {
                self.bump();
            }
            _ => return Err(self.err("expected `;` to close definition")),
        }

        // Embed nfst-xre.
        let body_spanned_xre = nfst_xre::parse(&body_str).map_err(|xre_err| {
            // Take the first xre diagnostic, shift its span into lexc source.
            let first = xre_err
                .diagnostics
                .into_iter()
                .next()
                .unwrap_or_else(|| Diagnostic::error(body_span.clone(), "xre parse failed"));
            let shifted = Span::anonymous(
                body_span.start() + first.span.start()..body_span.start() + first.span.end(),
            );
            Diagnostic::error(
                shifted,
                format!("error in xre definition body: {}", first.message),
            )
        })?;

        Ok(Spanned::new(
            Definition {
                name,
                body: body_spanned_xre,
            },
            self.merge(start),
        ))
    }

    fn parse_lexicon(&mut self) -> Result<Spanned<Lexicon>, Diagnostic> {
        let start = self.current_start();
        let (tok, _) = self.bump().unwrap();
        let (name, titlecase) = match tok {
            Token::LexiconStart { name, titlecase } => (name, titlecase),
            _ => unreachable!("called parse_lexicon without a LexiconStart"),
        };

        let mut entries = Vec::new();
        loop {
            match self.peek() {
                None | Some(Token::LexiconStart { .. }) | Some(Token::EndKeyword) => break,
                _ => {
                    let entry = self.parse_entry(&body_span_for_entry(self.peek_span()))?;
                    entries.push(entry);
                }
            }
        }

        Ok(Spanned::new(
            Lexicon {
                name,
                case_warning: titlecase,
                entries,
            },
            self.merge(start),
        ))
    }

    fn parse_entry(
        &mut self,
        _entry_span_hint: &Span,
    ) -> Result<Spanned<LexiconEntry>, Diagnostic> {
        let start = self.current_start();
        // Collect tokens that belong to this entry (everything up to the
        // next Semicolon — that Semicolon terminates the entry).
        let mut spec_tokens: Vec<(Token, Span)> = Vec::new();
        let mut continuation: Option<(SmolStr, Span)> = None;
        let mut gloss: Option<(SmolStr, Span)> = None;

        loop {
            match self.peek() {
                Some(Token::Semicolon) => {
                    self.bump();
                    break;
                }
                Some(Token::Quoted(_)) => {
                    let (tok, span) = self.bump().unwrap();
                    if let Token::Quoted(s) = tok {
                        gloss = Some((s, span));
                    }
                }
                Some(Token::Identifier(_)) => {
                    // Greedy: keep collecting; the last Identifier-token
                    // before the Semicolon (or before the Quoted gloss) is
                    // the continuation. We push to spec_tokens here and
                    // pop the last as continuation after the loop.
                    let item = self.bump().unwrap();
                    spec_tokens.push(item);
                }
                Some(Token::Colon) | Some(Token::XreBlock(_)) => {
                    let item = self.bump().unwrap();
                    spec_tokens.push(item);
                }
                Some(_) | None => {
                    return Err(self.err(format!(
                        "unexpected token {:?} inside lexicon entry",
                        self.peek()
                    )));
                }
            }
        }

        // The last Identifier in spec_tokens is the continuation. Walk
        // backwards.
        for i in (0..spec_tokens.len()).rev() {
            if let Token::Identifier(_) = &spec_tokens[i].0 {
                let (tok, span) = spec_tokens.remove(i);
                if let Token::Identifier(s) = tok {
                    continuation = Some((s, span));
                }
                break;
            }
        }

        let (continuation_name, _cont_span) = continuation.ok_or_else(|| {
            Diagnostic::error(
                self.merge(start),
                "lexicon entry missing a continuation name",
            )
        })?;

        let spec = self.classify_entry_spec(spec_tokens)?;

        Ok(Spanned::new(
            LexiconEntry {
                spec,
                continuation: continuation_name,
                gloss: gloss.map(|(s, _)| s),
            },
            self.merge(start),
        ))
    }

    /// Build an `EntrySpec` from the leftover spec tokens (after the
    /// continuation and gloss have been removed).
    fn classify_entry_spec(&self, toks: Vec<(Token, Span)>) -> Result<EntrySpec, Diagnostic> {
        let kinds: Vec<&Token> = toks.iter().map(|(t, _)| t).collect();
        match kinds.as_slice() {
            [] => Ok(EntrySpec::Empty),
            [Token::Identifier(s)] => Ok(EntrySpec::String(s.clone())),
            [Token::XreBlock(body)] => {
                let body_span = toks[0].1.clone();
                let xre = nfst_xre::parse(body).map_err(|xre_err| {
                    let first = xre_err.diagnostics.into_iter().next().unwrap_or_else(|| {
                        Diagnostic::error(body_span.clone(), "xre parse failed")
                    });
                    let body_start = body_span.start() + 1; // account for `<`
                    let shifted = Span::anonymous(
                        body_start + first.span.start()..body_start + first.span.end(),
                    );
                    Diagnostic::error(
                        shifted,
                        format!("error in xre lexicon entry: {}", first.message),
                    )
                })?;
                Ok(EntrySpec::Regex(xre))
            }
            [Token::Colon] => Ok(EntrySpec::Pair {
                upper: SmolStr::default(),
                lower: SmolStr::default(),
            }),
            [Token::Identifier(u), Token::Colon] => Ok(EntrySpec::Pair {
                upper: u.clone(),
                lower: SmolStr::default(),
            }),
            [Token::Colon, Token::Identifier(l)] => Ok(EntrySpec::Pair {
                upper: SmolStr::default(),
                lower: l.clone(),
            }),
            [Token::Identifier(u), Token::Colon, Token::Identifier(l)] => Ok(EntrySpec::Pair {
                upper: u.clone(),
                lower: l.clone(),
            }),
            _ => {
                let span = toks
                    .first()
                    .map(|(_, s)| s.clone())
                    .unwrap_or_else(|| self.peek_span());
                Err(Diagnostic::error(
                    span,
                    format!("could not interpret entry spec from tokens: {:?}", kinds),
                ))
            }
        }
    }
}

fn body_span_for_entry(s: Span) -> Span {
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::EntrySpec;

    fn parsed(src: &str) -> LexcFile {
        parse(src)
            .unwrap_or_else(|e| panic!("parse failed: {e:?}"))
            .value
    }

    #[test]
    fn smallest_valid_file() {
        let f = parsed("LEXICON Root\ndog # ;");
        assert_eq!(f.lexicons.len(), 1);
        assert_eq!(f.lexicons[0].value.name, "Root");
        assert_eq!(f.lexicons[0].value.entries.len(), 1);
        let e = &f.lexicons[0].value.entries[0].value;
        assert!(matches!(e.spec, EntrySpec::String(ref s) if s == "dog"));
        assert_eq!(e.continuation, "#");
    }

    #[test]
    fn pair_entry() {
        let f = parsed("LEXICON Root\ncat:dog # ;");
        let e = &f.lexicons[0].value.entries[0].value;
        match &e.spec {
            EntrySpec::Pair { upper, lower } => {
                assert_eq!(upper, "cat");
                assert_eq!(lower, "dog");
            }
            other => panic!("expected Pair, got {other:?}"),
        }
    }

    #[test]
    fn multichar_section() {
        let f = parsed("Multichar_Symbols +Sg +Pl\nLEXICON Root\nx # ;");
        assert_eq!(f.multichars.len(), 2);
        assert_eq!(f.multichars[0].value.0, "+Sg");
    }

    #[test]
    fn definition_section_with_embedded_xre() {
        let f = parsed("Definitions\nVowel = a | e | i ;\nLEXICON Root\nx # ;");
        assert_eq!(f.definitions.len(), 1);
        assert_eq!(f.definitions[0].value.name, "Vowel");
        // The xre body should be a Union expression.
        let body = &f.definitions[0].value.body.value;
        assert!(matches!(
            body,
            nfst_xre::XreExpr::Binary(nfst_xre::BinaryOp::Union, _, _)
        ));
    }

    #[test]
    fn xre_block_entry() {
        let f = parsed("LEXICON Root\n<a b c> # ;");
        match &f.lexicons[0].value.entries[0].value.spec {
            EntrySpec::Regex(xre) => {
                assert!(matches!(
                    xre.value,
                    nfst_xre::XreExpr::Binary(nfst_xre::BinaryOp::Concatenate, _, _)
                ));
            }
            other => panic!("expected Regex, got {other:?}"),
        }
    }

    #[test]
    fn entry_with_gloss() {
        let f = parsed(
            r#"LEXICON Root
dog Num "the dog" ;"#,
        );
        let e = &f.lexicons[0].value.entries[0].value;
        assert_eq!(e.gloss.as_deref(), Some("the dog"));
        assert_eq!(e.continuation, "Num");
    }

    #[test]
    fn end_marker_recorded() {
        let f = parsed("LEXICON Root\ndog # ;\nEND\n");
        assert!(f.has_end);
    }
}
