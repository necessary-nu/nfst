//! Hand-rolled stateful lexer for lexc. logos doesn't have explicit modes,
//! and lexc's grammar is genuinely sectioned (5 flex states); a small
//! state machine here is much clearer than fighting `Extras`.
//!
//! Section transitions:
//!
//!   Initial       ─Multichar_Symbols/Alphabets→  Multichars
//!   Initial       ─NOFLAGS→                       NoFlags
//!   Initial       ─Definitions→                   Definitions
//!   Initial       ─LEXICON name→                  Lexicons
//!   Multichars    ─NOFLAGS/Definitions/LEXICON→   {NoFlags,Definitions,Lexicons}
//!   NoFlags       ─;→                             Initial (then re-enters)
//!   NoFlags       ─Definitions/LEXICON→           {Definitions,Lexicons}
//!   Definitions   ─LEXICON→                       Lexicons
//!   Lexicons      ─LEXICON name→                  Lexicons (new lexicon)
//!   {any}         ─END→                           Ended (drops remaining input)
//!
//! NAME_CH: any printable ASCII except space `<%!;:"`, plus any non-ASCII
//! Unicode codepoint, plus `%X` (escape). Wider than xre's NAME_CH.

use crate::token::Token;
use nfst_syntax::Span;
use smol_str::{SmolStr, SmolStrBuilder};

#[derive(Clone, Debug, PartialEq)]
pub struct LexError {
    pub span: Span,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Section {
    #[default]
    Initial,
    Multichars,
    NoFlags,
    Definitions,
    Lexicons,
    Ended,
}

pub fn tokenize(source: &str) -> Result<Vec<(Token, Span)>, Vec<LexError>> {
    let mut lex = Lexer::new(source);
    lex.run();
    if lex.errors.is_empty() {
        Ok(lex.tokens)
    } else {
        Err(lex.errors)
    }
}

struct Lexer<'a> {
    source: &'a str,
    pos: usize,
    section: Section,
    tokens: Vec<(Token, Span)>,
    errors: Vec<LexError>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
            section: Section::default(),
            tokens: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn run(&mut self) {
        loop {
            self.skip_ws_and_comments();
            if self.is_at_end() {
                break;
            }
            if self.section == Section::Ended {
                self.pos = self.source.len();
                break;
            }
            let before = self.pos;
            self.step();
            if self.pos == before {
                // Couldn't make progress; record an error and skip a byte
                // to avoid infinite-looping. UTF-8-safe step.
                let span = Span::anonymous(self.pos..self.pos + 1);
                self.errors.push(LexError {
                    span,
                    message: format!(
                        "lexer stuck at byte {}: {:?}",
                        self.pos,
                        &self.source[self.pos..self.source.len().min(self.pos + 8)]
                    ),
                });
                self.advance_one_char();
            }
        }
    }

    // ───────────────────────── primitives ─────────────────────────

    fn is_at_end(&self) -> bool {
        self.pos >= self.source.len()
    }

    fn rest(&self) -> &str {
        &self.source[self.pos..]
    }

    fn peek_byte(&self) -> Option<u8> {
        self.source.as_bytes().get(self.pos).copied()
    }

    fn advance_one_char(&mut self) {
        let rest = self.rest();
        let mut chars = rest.char_indices();
        chars.next();
        let next = chars.next().map(|(i, _)| i).unwrap_or(rest.len());
        self.pos += next;
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            let bytes = self.source.as_bytes();
            // whitespace
            while self.pos < bytes.len() && matches!(bytes[self.pos], b' ' | b'\t' | b'\r' | b'\n')
            {
                self.pos += 1;
            }
            // line comment: `!.*\n`
            if self.pos < bytes.len() && bytes[self.pos] == b'!' {
                while self.pos < bytes.len() && bytes[self.pos] != b'\n' {
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
    }

    /// True when source[pos..pos+s.len()] == s and the following byte
    /// (if any) is not a NAME_CH (so the keyword has a word boundary).
    fn match_word(&self, s: &str) -> bool {
        if !self.rest().starts_with(s) {
            return false;
        }
        let after = self.pos + s.len();
        match self.source.as_bytes().get(after) {
            None => true,
            Some(&b) => is_word_boundary_byte(b),
        }
    }

    /// Consume a literal that must be followed by a NAME_CH word boundary.
    /// Returns the byte range consumed.
    fn consume_word(&mut self, s: &str) -> Option<Span> {
        if self.match_word(s) {
            let start = self.pos;
            self.pos += s.len();
            Some(Span::anonymous(start..self.pos))
        } else {
            None
        }
    }

    fn consume_byte(&mut self, b: u8) -> Option<Span> {
        if self.peek_byte() == Some(b) {
            let start = self.pos;
            self.pos += 1;
            Some(Span::anonymous(start..self.pos))
        } else {
            None
        }
    }

    // ───────────────────────── per-section dispatch ─────────────────────────

    fn step(&mut self) {
        match self.section {
            Section::Initial => self.step_initial(),
            Section::Multichars => self.step_multichars(),
            Section::NoFlags => self.step_noflags(),
            Section::Definitions => self.step_definitions(),
            Section::Lexicons => self.step_lexicons(),
            Section::Ended => unreachable!("Ended handled in run()"),
        }
    }

    fn step_initial(&mut self) {
        if self.try_section_starter() {
            return;
        }
        self.error(
            "expected section header (Multichar_Symbols, NOFLAGS, Definitions, LEXICON, END)",
        );
    }

    fn step_multichars(&mut self) {
        if self.try_section_starter() {
            return;
        }
        // Otherwise lex an identifier as a multichar symbol.
        if let Some((id, span)) = self.lex_identifier() {
            self.tokens.push((Token::Identifier(id), span));
        } else {
            self.error("expected a multichar symbol or section header");
        }
    }

    fn step_noflags(&mut self) {
        // `;` ends the NOFLAGS section and returns control to Initial.
        if let Some(span) = self.consume_byte(b';') {
            self.tokens.push((Token::Semicolon, span));
            self.section = Section::Initial;
            return;
        }
        if self.try_section_starter() {
            return;
        }
        if let Some((id, span)) = self.lex_identifier() {
            self.tokens.push((Token::Identifier(id), span));
        } else {
            self.error("expected a lexicon name or `;` to end NOFLAGS");
        }
    }

    fn step_definitions(&mut self) {
        if self.try_section_starter() {
            return;
        }
        // Definition line: `name = body ;`
        let Some((name, name_span)) = self.lex_identifier() else {
            self.error("expected a definition name");
            return;
        };
        self.tokens.push((Token::Identifier(name), name_span));

        self.skip_ws_and_comments();

        let Some(eq_span) = self.consume_byte(b'=') else {
            self.error("expected `=` after definition name");
            return;
        };
        self.tokens.push((Token::Equals, eq_span));

        // Capture body until `;`, respecting `"…"` quotes.
        let body_start = self.pos;
        let body = self.scan_definition_body();
        let body_span = Span::anonymous(body_start..self.pos);
        self.tokens
            .push((Token::DefinitionBody(body.trim().into()), body_span));

        if let Some(semi) = self.consume_byte(b';') {
            self.tokens.push((Token::Semicolon, semi));
        } else {
            self.error("expected `;` to terminate definition body");
        }
    }

    fn step_lexicons(&mut self) {
        // Section transitions (LEXICON / END) come first.
        if let Some(span) = self.consume_word("END") {
            self.tokens.push((Token::EndKeyword, span));
            self.section = Section::Ended;
            return;
        }
        if let Some((tok, span)) = self.try_lexicon_start() {
            self.tokens.push((tok, span));
            return;
        }
        // Structural punctuation.
        match self.peek_byte() {
            Some(b';') => {
                let span = self.consume_byte(b';').unwrap();
                self.tokens.push((Token::Semicolon, span));
            }
            Some(b':') => {
                let span = self.consume_byte(b':').unwrap();
                self.tokens.push((Token::Colon, span));
            }
            Some(b'<') => self.lex_xre_block(),
            Some(b'"') => self.lex_quoted(),
            _ => {
                if let Some((id, span)) = self.lex_identifier() {
                    self.tokens.push((Token::Identifier(id), span));
                } else {
                    self.error("expected a lexicon entry, LEXICON header, or END");
                }
            }
        }
    }

    // ───────────────────────── section starters ─────────────────────────

    /// Try every possible section-start keyword from the current state.
    /// Returns true if one fired.
    fn try_section_starter(&mut self) -> bool {
        if let Some(span) = self.consume_word("Multichar_Symbols") {
            self.tokens
                .push((Token::SectionMulticharsStart { alphabets: false }, span));
            self.section = Section::Multichars;
            return true;
        }
        if let Some(span) = self.consume_word("MULTICHAR_SYMBOLS") {
            self.tokens
                .push((Token::SectionMulticharsStart { alphabets: false }, span));
            self.section = Section::Multichars;
            return true;
        }
        if let Some(span) = self.consume_word("Alphabets") {
            self.tokens
                .push((Token::SectionMulticharsStart { alphabets: true }, span));
            self.section = Section::Multichars;
            return true;
        }
        if let Some(span) = self.consume_word("ALPHABETS") {
            self.tokens
                .push((Token::SectionMulticharsStart { alphabets: true }, span));
            self.section = Section::Multichars;
            return true;
        }
        if let Some(span) = self.consume_word("NOFLAGS") {
            self.tokens.push((Token::SectionNoFlagsStart, span));
            self.section = Section::NoFlags;
            return true;
        }
        if let Some(span) = self.consume_word("NoFlags") {
            self.tokens.push((Token::SectionNoFlagsStart, span));
            self.section = Section::NoFlags;
            return true;
        }
        for kw in &["Definitions", "Declarations", "DEFINITIONS", "DECLARATIONS"] {
            if let Some(span) = self.consume_word(kw) {
                self.tokens.push((Token::SectionDefinitionsStart, span));
                self.section = Section::Definitions;
                return true;
            }
        }
        if let Some((tok, span)) = self.try_lexicon_start() {
            self.tokens.push((tok, span));
            return true;
        }
        if let Some(span) = self.consume_word("END") {
            self.tokens.push((Token::EndKeyword, span));
            self.section = Section::Ended;
            return true;
        }
        false
    }

    fn try_lexicon_start(&mut self) -> Option<(Token, Span)> {
        let start = self.pos;
        let titlecase = if self.match_word("LEXICON") {
            self.pos += "LEXICON".len();
            false
        } else if self.match_word("Lexicon") {
            self.pos += "Lexicon".len();
            true
        } else {
            return None;
        };
        // Require at least one whitespace, then a name.
        let ws_start = self.pos;
        while matches!(self.peek_byte(), Some(b' ') | Some(b'\t')) {
            self.pos += 1;
        }
        if self.pos == ws_start {
            // No whitespace after keyword — back out; let it be an identifier.
            self.pos = start;
            return None;
        }
        let Some((name, _)) = self.lex_identifier() else {
            self.pos = start;
            return None;
        };
        let span = Span::anonymous(start..self.pos);
        self.section = Section::Lexicons;
        Some((Token::LexiconStart { name, titlecase }, span))
    }

    // ───────────────────────── value scanners ─────────────────────────

    fn lex_identifier(&mut self) -> Option<(SmolStr, Span)> {
        let start = self.pos;
        let mut out = SmolStrBuilder::new();
        loop {
            let bytes = self.source.as_bytes();
            if self.pos >= bytes.len() {
                break;
            }
            let b = bytes[self.pos];
            // Escape: `%X` consumes two characters; the X is kept. This mirrors
            // upstream `hfst::lexc::strip_percents(s, do_zeros=false)`: every
            // escape is unescaped to its literal, EXCEPT `%0`, which becomes the
            // `@ZERO@` marker. That marker is what distinguishes an escaped
            // literal zero (kept as "0" downstream) from a bare `0` (which the
            // lexc compiler reads as epsilon). Collapsing `%0` to a bare `0`
            // here would silently turn literal zeros into epsilons.
            if b == b'%' {
                self.pos += 1;
                let rest = self.rest();
                if let Some(c) = rest.chars().next() {
                    if c == '0' {
                        out.push_str("@ZERO@");
                    } else {
                        out.push(c);
                    }
                    self.pos += c.len_utf8();
                    continue;
                } else {
                    // trailing `%` — accept literally
                    out.push('%');
                    continue;
                }
            }
            // ASCII printable except space and `<%!;:"`
            if (0x21..=0x7e).contains(&b) {
                if matches!(b, b'<' | b'!' | b';' | b':' | b'"') {
                    break;
                }
                out.push(b as char);
                self.pos += 1;
                continue;
            }
            // High-bit byte: decode UTF-8 codepoint.
            if b >= 0x80 {
                let rest = self.rest();
                if let Some(c) = rest.chars().next() {
                    out.push(c);
                    self.pos += c.len_utf8();
                    continue;
                }
            }
            break;
        }
        if self.pos == start {
            None
        } else {
            Some((out.finish(), Span::anonymous(start..self.pos)))
        }
    }

    fn lex_xre_block(&mut self) {
        let start = self.pos;
        debug_assert_eq!(self.peek_byte(), Some(b'<'));
        self.pos += 1; // consume `<`

        let body_start = self.pos;
        let mut in_quote = false;
        let bytes = self.source.as_bytes();
        while self.pos < bytes.len() {
            let b = bytes[self.pos];
            let next = bytes.get(self.pos + 1).copied();
            match b {
                b'"' => {
                    in_quote = !in_quote;
                    self.pos += 1;
                }
                // xre 2-char operators containing `>` or `<`. Mirrors the
                // upstream `XREOPERATOR` set: `<>`, `^>`, `^<`. Without
                // this, the embedded `^>2` (CatenateNPlus) eats its own
                // `>` as the block terminator.
                b'<' if !in_quote && next == Some(b'>') => {
                    self.pos += 2;
                }
                b'^' if !in_quote && (next == Some(b'>') || next == Some(b'<')) => {
                    self.pos += 2;
                }
                // lexc `%` escape: `%X` is a literal X, never syntax. In
                // particular `%>` is an escaped boundary symbol `>`, not the
                // block terminator (lang-sme `< "+Nom":%> … >` entries).
                b'%' if !in_quote => {
                    self.pos += 2;
                }
                b'>' if !in_quote => {
                    let body: SmolStr = self.source[body_start..self.pos].trim().into();
                    self.pos += 1; // consume `>`
                    self.tokens
                        .push((Token::XreBlock(body), Span::anonymous(start..self.pos)));
                    return;
                }
                _ => self.pos += 1,
            }
        }
        // ran off end without `>`
        self.errors.push(LexError {
            span: Span::anonymous(start..self.pos),
            message: "unterminated `<…>` xre block".to_string(),
        });
    }

    fn lex_quoted(&mut self) {
        let start = self.pos;
        debug_assert_eq!(self.peek_byte(), Some(b'"'));
        self.pos += 1; // consume opening `"`

        let body_start = self.pos;
        let bytes = self.source.as_bytes();
        while self.pos < bytes.len() {
            let b = bytes[self.pos];
            match b {
                b'"' => {
                    let body: SmolStr = self.source[body_start..self.pos].into();
                    self.pos += 1; // consume closing `"`
                    self.tokens
                        .push((Token::Quoted(body), Span::anonymous(start..self.pos)));
                    return;
                }
                b'\n' => break, // strings can't span lines (matches upstream)
                _ => self.pos += 1,
            }
        }
        self.errors.push(LexError {
            span: Span::anonymous(start..self.pos),
            message: "unterminated quoted gloss".to_string(),
        });
    }

    /// Scan from the current position until (but not including) the next
    /// `;` that lies outside of a `"…"` quoted string. Returns the body
    /// substring.
    fn scan_definition_body(&mut self) -> SmolStr {
        let start = self.pos;
        let mut in_quote = false;
        let bytes = self.source.as_bytes();
        while self.pos < bytes.len() {
            let b = bytes[self.pos];
            match b {
                b'"' => {
                    in_quote = !in_quote;
                    self.pos += 1;
                }
                b';' if !in_quote => break,
                b'\\' if self.pos + 1 < bytes.len() => {
                    // escape: skip the next byte too
                    self.pos += 2;
                }
                _ => self.pos += 1,
            }
        }
        self.source[start..self.pos].into()
    }

    // ───────────────────────── error helper ─────────────────────────

    fn error(&mut self, msg: impl Into<String>) {
        let span = Span::anonymous(self.pos..self.pos + 1);
        self.errors.push(LexError {
            span,
            message: msg.into(),
        });
        self.advance_one_char();
    }
}

fn is_word_boundary_byte(b: u8) -> bool {
    // A NAME_CH boundary is anything that isn't a NAME_CH continuation.
    // NAME_CH includes printable ASCII (minus the special set), high-bit
    // UTF-8, and the `%` escape prefix.
    matches!(
        b,
        b' ' | b'\t' | b'\r' | b'\n' | b'<' | b'!' | b';' | b':' | b'"'
    )
}

// ───────────────────────── tests ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(src: &str) -> Vec<Token> {
        tokenize(src)
            .expect("clean lex")
            .into_iter()
            .map(|(t, _)| t)
            .collect()
    }

    #[test]
    fn empty_input() {
        assert!(lex("").is_empty());
    }

    #[test]
    fn just_a_lexicon() {
        let toks = lex("LEXICON Root\ndog # ;\n");
        assert!(matches!(
            toks[0],
            Token::LexiconStart { ref name, titlecase: false } if name == "Root"
        ));
        assert_eq!(toks[1], Token::Identifier("dog".into()));
        assert_eq!(toks[2], Token::Identifier("#".into()));
        assert_eq!(toks[3], Token::Semicolon);
    }

    #[test]
    fn multichar_symbols_section() {
        let toks = lex("Multichar_Symbols +Sg +Pl\n\nLEXICON Root\ndog # ;");
        assert_eq!(toks[0], Token::SectionMulticharsStart { alphabets: false });
        assert_eq!(toks[1], Token::Identifier("+Sg".into()));
        assert_eq!(toks[2], Token::Identifier("+Pl".into()));
        assert!(matches!(toks[3], Token::LexiconStart { .. }));
    }

    #[test]
    fn alphabets_section_is_strict() {
        let toks = lex("Alphabets a b c\nLEXICON Root\nx # ;");
        assert_eq!(toks[0], Token::SectionMulticharsStart { alphabets: true });
    }

    #[test]
    fn comments_skipped() {
        let toks = lex("! comment\nLEXICON Root\n! another\ndog # ;");
        assert!(matches!(toks[0], Token::LexiconStart { .. }));
    }

    #[test]
    fn pair_entry() {
        let toks = lex("LEXICON Root\ncat:dog # ;");
        // After the LexiconStart: cat, :, dog, #, ;
        assert_eq!(toks[1], Token::Identifier("cat".into()));
        assert_eq!(toks[2], Token::Colon);
        assert_eq!(toks[3], Token::Identifier("dog".into()));
        assert_eq!(toks[4], Token::Identifier("#".into()));
        assert_eq!(toks[5], Token::Semicolon);
    }

    #[test]
    fn xre_block_in_lexicon() {
        let toks = lex("LEXICON Root\n<a b c> END ;");
        // LexiconStart, XreBlock("a b c"), Identifier("END"... wait END is keyword).
        // Actually END is a keyword and switches to Ended state; the trailing ;
        // is in Ended state and will be skipped.
        assert!(matches!(toks[0], Token::LexiconStart { .. }));
        assert_eq!(toks[1], Token::XreBlock("a b c".into()));
        assert_eq!(toks[2], Token::EndKeyword);
    }

    #[test]
    fn definition_line() {
        let toks = lex("Definitions\nVowel = a | e | i ;\n\nLEXICON Root\nx # ;");
        assert_eq!(toks[0], Token::SectionDefinitionsStart);
        assert_eq!(toks[1], Token::Identifier("Vowel".into()));
        assert_eq!(toks[2], Token::Equals);
        assert_eq!(toks[3], Token::DefinitionBody("a | e | i".into()));
        assert_eq!(toks[4], Token::Semicolon);
        assert!(matches!(toks[5], Token::LexiconStart { .. }));
    }

    #[test]
    fn quoted_gloss() {
        let toks = lex(r#"LEXICON Root
dog Num "tag" ;"#);
        assert!(matches!(toks[0], Token::LexiconStart { .. }));
        assert_eq!(toks[1], Token::Identifier("dog".into()));
        assert_eq!(toks[2], Token::Identifier("Num".into()));
        assert_eq!(toks[3], Token::Quoted("tag".into()));
        assert_eq!(toks[4], Token::Semicolon);
    }

    #[test]
    fn percent_escapes_in_identifiers() {
        // `%+N` strips to `+N`.
        let toks = lex("LEXICON Root\n%+N # ;");
        assert_eq!(toks[1], Token::Identifier("+N".into()));
    }

    #[test]
    fn escaped_zero_becomes_zero_marker() {
        // `%0` is an escaped literal zero, distinct from a bare `0` (epsilon).
        // It lexes to the `@ZERO@` marker, matching upstream
        // strip_percents(do_zeros=false); a bare `0` stays `0`.
        let toks = lex("LEXICON Root\n%0 ARABIC ;");
        assert_eq!(toks[1], Token::Identifier("@ZERO@".into()));
        let toks = lex("LEXICON Root\n0 ARABIC ;");
        assert_eq!(toks[1], Token::Identifier("0".into()));
        // Escaped zero inside a larger token: `1%0` -> `1@ZERO@`.
        let toks = lex("LEXICON Root\n1%0 ARABIC ;");
        assert_eq!(toks[1], Token::Identifier("1@ZERO@".into()));
    }

    #[test]
    fn end_terminates_lexer() {
        let toks = lex("LEXICON Root\ndog # ;\nEND\nblah blah blah");
        // After END, everything is skipped.
        assert!(toks.iter().any(|t| matches!(t, Token::EndKeyword)));
        // `blah` never appears as an identifier:
        assert!(
            toks.iter()
                .all(|t| !matches!(t, Token::Identifier(s) if s == "blah"))
        );
    }

    #[test]
    fn lowercase_lexicon_keyword() {
        let toks = lex("Lexicon Root\ndog # ;");
        assert!(matches!(
            toks[0],
            Token::LexiconStart { ref name, titlecase: true } if name == "Root"
        ));
    }
}
