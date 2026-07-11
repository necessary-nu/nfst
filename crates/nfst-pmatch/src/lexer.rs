//! Hand-rolled pmatch lexer. The grammar's keyword surface (~30 function-
//! call prefixes, 6 acceptors, 11 variable names, full xre operator
//! suite, four `:` variants) outpaces what logos can model cleanly, so a
//! direct scanner stays both more readable and easier to evolve.
//!
//! Single pass over UTF-8 bytes. The `last_skipped_whitespace` flag lets
//! the colon disambiguator pick the right `PAIR_SEPARATOR_*` variant by
//! looking at "did we just skip whitespace?" plus "is the next byte
//! whitespace?".

use crate::token::Token;
use nfst_syntax::Span;
use smol_str::{SmolStr, SmolStrBuilder};

#[derive(Clone, Debug, PartialEq)]
pub struct LexError {
    pub span: Span,
    pub message: String,
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

const VARIABLE_NAMES: &[&str] = &[
    "count-patterns",
    "delete-patterns",
    "extract-patterns",
    "locate-patterns",
    "mark-patterns",
    "need-separators",
    "unicode-character-classes",
    "max-context-length",
    "max-recursion",
    "xerox-composition",
    "vector-similarity-projection-factor",
];

struct Lexer<'a> {
    source: &'a str,
    pos: usize,
    last_skipped_whitespace: bool,
    tokens: Vec<(Token, Span)>,
    errors: Vec<LexError>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
            // Start of file counts as "after whitespace" for `:` purposes.
            last_skipped_whitespace: true,
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
            let before = self.pos;
            self.step();
            if self.pos == before {
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
            // Whitespace handling for `:` variants: the next call to step()
            // will set last_skipped_whitespace based on whether
            // skip_ws_and_comments did any work. We don't need to clear it
            // here.
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

    fn peek_byte_at(&self, off: usize) -> Option<u8> {
        self.source.as_bytes().get(self.pos + off).copied()
    }

    fn advance_one_char(&mut self) {
        let rest = self.rest();
        let mut chars = rest.char_indices();
        chars.next();
        let next = chars.next().map(|(i, _)| i).unwrap_or(rest.len());
        self.pos += next;
    }

    fn skip_ws_and_comments(&mut self) {
        let start = self.pos;
        loop {
            let bytes = self.source.as_bytes();
            while self.pos < bytes.len() && matches!(bytes[self.pos], b' ' | b'\t' | b'\r' | b'\n')
            {
                self.pos += 1;
            }
            if self.pos < bytes.len() && bytes[self.pos] == b'!' {
                while self.pos < bytes.len() && bytes[self.pos] != b'\n' {
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
        self.last_skipped_whitespace = self.pos > start;
    }

    fn match_str(&self, s: &str) -> bool {
        self.rest().starts_with(s)
    }

    fn match_word(&self, s: &str) -> bool {
        if !self.rest().starts_with(s) {
            return false;
        }
        let after = self.pos + s.len();
        match self.source.as_bytes().get(after) {
            None => true,
            Some(&b) => !is_name_char_byte(b),
        }
    }

    fn consume(&mut self, s: &str) -> Option<Span> {
        if self.rest().starts_with(s) {
            let start = self.pos;
            self.pos += s.len();
            Some(Span::anonymous(start..self.pos))
        } else {
            None
        }
    }

    fn emit(&mut self, tok: Token, span: Span) {
        self.tokens.push((tok, span));
    }

    fn error(&mut self, msg: impl Into<String>) {
        let span = Span::anonymous(self.pos..self.pos + 1);
        self.errors.push(LexError {
            span,
            message: msg.into(),
        });
    }

    // ───────────────────────── dispatcher ─────────────────────────

    fn step(&mut self) {
        // 1. Multi-char operators starting with `.`.
        if self.peek_byte() == Some(b'.') && self.try_dot_starting().is_some() {
            return;
        }

        // 2. Catenation N (`^...`).
        if self.peek_byte() == Some(b'^') && self.try_catenate_n().is_some() {
            return;
        }

        // 3. @-forms.
        if self.peek_byte() == Some(b'@') && self.try_at_form().is_some() {
            return;
        }

        // 4. `;` (with optional weight).
        if self.peek_byte() == Some(b';') {
            self.lex_semicolon();
            return;
        }

        // 5. `::weight`.
        if self.match_str("::") && is_weight_start(self.peek_byte_at(2)) {
            self.lex_weight();
            return;
        }

        // 6. `:` variants.
        if self.peek_byte() == Some(b':') {
            self.lex_colon();
            return;
        }

        // 7. Multi-char operators (sorted longest-first within each prefix).
        if let Some(()) = self.try_long_operator() {
            return;
        }

        // 8. Single-char structural and operators.
        if let Some(()) = self.try_single_char() {
            return;
        }

        // 9. Quoted literal `"..."` (also character ranges).
        if self.peek_byte() == Some(b'"') {
            self.lex_quoted_or_range();
            return;
        }

        // 10. Curly literal `{...}`.
        if self.peek_byte() == Some(b'{') {
            self.lex_curly();
            return;
        }

        // 11. Variable names (compound keywords containing `-`).
        if let Some(()) = self.try_variable_name_keyword() {
            return;
        }

        // 12. Identifier or keyword.
        self.lex_identifier_or_keyword();
    }

    // ───────────────────────── dot-prefixed ─────────────────────────

    fn try_dot_starting(&mut self) -> Option<()> {
        // `.#.` is BOUNDARY_MARKER (3 chars).
        if let Some(span) = self.consume(".#.") {
            self.emit(Token::BoundaryMarker, span);
            return Some(());
        }
        // 4-char operators
        if let Some(span) = self.consume(".m>.") {
            self.emit(Token::MergeRightArrow, span);
            return Some(());
        }
        if let Some(span) = self.consume(".<m.") {
            self.emit(Token::MergeLeftArrow, span);
            return Some(());
        }
        if let Some(span) = self.consume(".-u.") {
            self.emit(Token::UpperMinus, span);
            return Some(());
        }
        if let Some(span) = self.consume(".-l.") {
            self.emit(Token::LowerMinus, span);
            return Some(());
        }
        // 3-char operators
        if let Some(span) = self.consume(".o.") {
            self.emit(Token::Composition, span);
            return Some(());
        }
        if let Some(span) = self.consume(".O.") {
            self.emit(Token::LenientComposition, span);
            return Some(());
        }
        if let Some(span) = self.consume(".x.") {
            self.emit(Token::CrossProduct, span);
            return Some(());
        }
        if let Some(span) = self.consume(".P.") {
            self.emit(Token::UpperPriorityUnion, span);
            return Some(());
        }
        if let Some(span) = self.consume(".p.") {
            self.emit(Token::LowerPriorityUnion, span);
            return Some(());
        }
        if let Some(span) = self.consume("./.") {
            self.emit(Token::IgnoreInternally, span);
            return Some(());
        }
        // .with( / .tag( / .t(
        if let Some(span) = self.consume(".with(") {
            self.emit(Token::WithLeft, span);
            return Some(());
        }
        if let Some(span) = self.consume(".tag(") {
            self.emit(Token::TagLeft, span);
            return Some(());
        }
        if let Some(span) = self.consume(".t(") {
            self.emit(Token::TagLeft, span);
            return Some(());
        }
        // 2-char projections / inversions
        if let Some(span) = self.consume(".r") {
            self.emit(Token::Reverse, span);
            return Some(());
        }
        if let Some(span) = self.consume(".i") {
            self.emit(Token::Invert, span);
            return Some(());
        }
        if let Some(span) = self.consume(".u") {
            self.emit(Token::UpperProject, span);
            return Some(());
        }
        if let Some(span) = self.consume(".l") {
            self.emit(Token::LowerProject, span);
            return Some(());
        }
        None
    }

    // ───────────────────────── catenation N ─────────────────────────

    fn try_catenate_n(&mut self) -> Option<()> {
        // Possible: `^{N,K}`, `^N,K`, `^>N`, `^<N`, `^N`.
        let start = self.pos;
        let after_caret = self.pos + 1;
        let next = *self.source.as_bytes().get(after_caret)?;
        // `^{N,K}` form.
        if next == b'{' {
            // scan `{N,K}`
            let scan_start = after_caret + 1;
            let (n, after_n) = parse_uint(self.source.as_bytes(), scan_start)?;
            if self.source.as_bytes().get(after_n) != Some(&b',') {
                return None;
            }
            let (k, after_k) = parse_uint(self.source.as_bytes(), after_n + 1)?;
            if self.source.as_bytes().get(after_k) != Some(&b'}') {
                return None;
            }
            let end = after_k + 1;
            self.pos = end;
            self.emit(Token::CatenateNToK(n, k), Span::anonymous(start..end));
            return Some(());
        }
        // `^>N` and `^<N`.
        if next == b'>' {
            let (n, after) = parse_uint(self.source.as_bytes(), after_caret + 1)?;
            self.pos = after;
            self.emit(Token::CatenateNPlus(n), Span::anonymous(start..after));
            return Some(());
        }
        if next == b'<' {
            let (n, after) = parse_uint(self.source.as_bytes(), after_caret + 1)?;
            self.pos = after;
            self.emit(Token::CatenateNMinus(n), Span::anonymous(start..after));
            return Some(());
        }
        // `^N` or `^N,K`.
        let (n, after_n) = parse_uint(self.source.as_bytes(), after_caret)?;
        if self.source.as_bytes().get(after_n) == Some(&b',') {
            let (k, after_k) = parse_uint(self.source.as_bytes(), after_n + 1)?;
            self.pos = after_k;
            self.emit(Token::CatenateNToK(n, k), Span::anonymous(start..after_k));
            return Some(());
        }
        self.pos = after_n;
        self.emit(Token::CatenateN(n), Span::anonymous(start..after_n));
        Some(())
    }

    // ───────────────────────── @-forms ─────────────────────────

    fn try_at_form(&mut self) -> Option<()> {
        // Try each prefix; on match, expect `"..."`.
        for (prefix, build) in [
            ("@bin\"", build_read_bin as fn(SmolStr) -> Token),
            ("@txt\"", build_read_text),
            ("@stxt\"", build_read_spaced),
            ("@pl\"", build_read_prolog),
            ("@lexc\"", build_read_lexc),
            ("@re\"", build_read_re),
            ("@vec\"", build_read_vec),
            ("@\"", build_read_bin), // bare `@"…"` → ReadBin
        ] {
            if self.match_str(prefix) {
                let start = self.pos;
                self.pos += prefix.len();
                let body_start = self.pos;
                let bytes = self.source.as_bytes();
                while self.pos < bytes.len() && bytes[self.pos] != b'"' {
                    self.pos += 1;
                }
                if self.pos >= bytes.len() {
                    self.errors.push(LexError {
                        span: Span::anonymous(start..self.pos),
                        message: format!("unterminated `{prefix}…` form"),
                    });
                    return Some(());
                }
                let path: SmolStr = self.source[body_start..self.pos].into();
                self.pos += 1; // consume closing `"`
                let span = Span::anonymous(start..self.pos);
                self.emit(build(path), span);
                return Some(());
            }
        }
        None
    }

    // ───────────────────────── semicolon (with optional weight) ─────────────────────────

    fn lex_semicolon(&mut self) {
        let start = self.pos;
        self.pos += 1;
        // Optional `<wsp>* ::weight`
        let save = self.pos;
        let bytes = self.source.as_bytes();
        while self.pos < bytes.len() && matches!(bytes[self.pos], b' ' | b'\t') {
            self.pos += 1;
        }
        if self.match_str("::") && is_weight_start(self.peek_byte_at(2)) {
            self.pos += 2;
            let w_start = self.pos;
            while self.pos < bytes.len() && is_weight_byte(bytes[self.pos]) {
                self.pos += 1;
            }
            let weight: f64 = self.source[w_start..self.pos].parse().unwrap_or(0.0);
            let span = Span::anonymous(start..self.pos);
            self.emit(Token::EndOfWeightedExpression(weight), span);
            return;
        }
        // No weight; rewind whitespace and emit zero-weight `;`.
        self.pos = save;
        self.emit(
            Token::EndOfWeightedExpression(0.0),
            Span::anonymous(start..save),
        );
    }

    // ───────────────────────── weight (`::W`) ─────────────────────────

    fn lex_weight(&mut self) {
        let start = self.pos;
        self.pos += 2; // `::`
        let bytes = self.source.as_bytes();
        let w_start = self.pos;
        while self.pos < bytes.len() && is_weight_byte(bytes[self.pos]) {
            self.pos += 1;
        }
        let weight: f64 = self.source[w_start..self.pos].parse().unwrap_or(0.0);
        self.emit(Token::Weight(weight), Span::anonymous(start..self.pos));
    }

    // ───────────────────────── colon disambiguation ─────────────────────────

    fn lex_colon(&mut self) {
        let start = self.pos;
        let prev_ws = self.last_skipped_whitespace;
        self.pos += 1;
        let next_ws = matches!(
            self.peek_byte(),
            Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')
        );
        let tok = match (prev_ws, next_ws) {
            (true, true) => Token::PairSeparatorSole,
            (true, false) => Token::PairSeparatorWoLeft,
            (false, true) => Token::PairSeparatorWoRight,
            (false, false) => Token::PairSeparator,
        };
        self.emit(tok, Span::anonymous(start..self.pos));
    }

    // ───────────────────────── multi-char operators ─────────────────────────

    fn try_long_operator(&mut self) -> Option<()> {
        // Order: longest first, ties resolved by appearance in pmatch_lex.ll.
        // 5 chars
        if let Some(span) = self.consume("(<->)") {
            self.emit(Token::OptionalReplaceLeftRight, span);
            return Some(());
        }
        // 4 chars
        if let Some(span) = self.consume("(->)") {
            self.emit(Token::OptionalReplaceRight, span);
            return Some(());
        }
        if let Some(span) = self.consume("(<-)") {
            self.emit(Token::OptionalReplaceLeft, span);
            return Some(());
        }
        // 3 chars
        if let Some(span) = self.consume("\\<=") {
            self.emit(Token::LeftRestriction, span);
            return Some(());
        }
        if let Some(span) = self.consume("<=>") {
            self.emit(Token::LeftRightArrow, span);
            return Some(());
        }
        if let Some(span) = self.consume("<->") {
            self.emit(Token::ReplaceLeftRight, span);
            return Some(());
        }
        if let Some(span) = self.consume("@->") {
            self.emit(Token::LtrLongestMatch, span);
            return Some(());
        }
        if let Some(span) = self.consume("->@") {
            self.emit(Token::RtlLongestMatch, span);
            return Some(());
        }
        if let Some(span) = self.consume("\\\\\\") {
            self.emit(Token::LeftQuotient, span);
            return Some(());
        }
        if let Some(span) = self.consume("\\//") {
            self.emit(Token::ReplaceContextLl, span);
            return Some(());
        }
        // 2 chars
        if let Some(span) = self.consume("<=") {
            self.emit(Token::LeftArrow, span);
            return Some(());
        }
        if let Some(span) = self.consume("=>") {
            self.emit(Token::RightArrow, span);
            return Some(());
        }
        if let Some(span) = self.consume("->") {
            self.emit(Token::ReplaceRight, span);
            return Some(());
        }
        if let Some(span) = self.consume("<-") {
            self.emit(Token::ReplaceLeft, span);
            return Some(());
        }
        if let Some(span) = self.consume(">@") {
            self.emit(Token::RtlShortestMatch, span);
            return Some(());
        }
        if let Some(span) = self.consume("@>") {
            self.emit(Token::LtrShortestMatch, span);
            return Some(());
        }
        if let Some(span) = self.consume("<>") {
            self.emit(Token::Shuffle, span);
            return Some(());
        }
        if let Some(span) = self.consume("||") {
            self.emit(Token::ReplaceContextUu, span);
            return Some(());
        }
        if let Some(span) = self.consume("//") {
            self.emit(Token::ReplaceContextLu, span);
            return Some(());
        }
        if let Some(span) = self.consume("\\\\") {
            self.emit(Token::ReplaceContextUl, span);
            return Some(());
        }
        if let Some(span) = self.consume(",,") {
            self.emit(Token::Commacomma, span);
            return Some(());
        }
        if let Some(span) = self.consume("[.") {
            self.emit(Token::LeftBracketDotted, span);
            return Some(());
        }
        if let Some(span) = self.consume(".]") {
            self.emit(Token::RightBracketDotted, span);
            return Some(());
        }
        if let Some(span) = self.consume("\"\"") {
            self.emit(Token::EpsilonToken, span);
            return Some(());
        }
        if let Some(span) = self.consume("[]") {
            self.emit(Token::EpsilonToken, span);
            return Some(());
        }
        // `_+` — one or more underscores → CenterMarker
        if self.peek_byte() == Some(b'_') {
            let start = self.pos;
            while self.peek_byte() == Some(b'_') {
                self.pos += 1;
            }
            self.emit(Token::CenterMarker, Span::anonymous(start..self.pos));
            return Some(());
        }
        // `...+` — three or more dots → MarkupMarker
        if self.match_str("...") {
            let start = self.pos;
            while self.match_str("...") {
                self.pos += 3;
            }
            self.emit(Token::MarkupMarker, Span::anonymous(start..self.pos));
            return Some(());
        }
        None
    }

    // ───────────────────────── single-char ─────────────────────────

    fn try_single_char(&mut self) -> Option<()> {
        let b = self.peek_byte()?;
        let span_for_one = || {
            let start = self.pos;
            Span::anonymous(start..start + 1)
        };
        let tok = match b {
            b'~' => Token::Complement,
            b'\\' => Token::TermComplement,
            b'&' => Token::Intersection,
            b'-' => Token::Minus,
            b'+' => Token::Plus,
            b'*' => Token::Star,
            b'|' => Token::Union,
            b'<' => Token::Before,
            b'>' => Token::After,
            b'/' => Token::Ignoring,
            b'?' => Token::AnyToken,
            b'`' => Token::SubstituteLeft,
            b'#' => Token::BoundaryMarker,
            b'[' => Token::LeftBracket,
            b']' => Token::RightBracket,
            b'(' => Token::LeftParenthesis,
            b')' => Token::RightParenthesis,
            b'=' => Token::Equals,
            b',' => Token::Comma,
            // CONTAINMENT and friends are tried here too (before `$` standalone).
            b'$' => {
                if self.peek_byte_at(1) == Some(b'.') {
                    self.pos += 2;
                    self.emit(
                        Token::ContainmentOnce,
                        Span::anonymous(self.pos - 2..self.pos),
                    );
                    return Some(());
                }
                if self.peek_byte_at(1) == Some(b'?') {
                    self.pos += 2;
                    self.emit(
                        Token::ContainmentOpt,
                        Span::anonymous(self.pos - 2..self.pos),
                    );
                    return Some(());
                }
                Token::Containment
            }
            b'0' => {
                // Plain `0` is EpsilonToken; `0` followed by NAME_CH chars is
                // a Symbol — fall through to identifier path.
                let next = self.peek_byte_at(1);
                if next.map(is_name_char_byte).unwrap_or(false) {
                    return None;
                }
                Token::EpsilonToken
            }
            _ => return None,
        };
        let span = span_for_one();
        self.pos += 1;
        self.emit(tok, span);
        Some(())
    }

    // ───────────────────────── quoted / curly ─────────────────────────

    fn lex_quoted_or_range(&mut self) {
        let start = self.pos;
        debug_assert_eq!(self.peek_byte(), Some(b'"'));
        self.pos += 1;
        let body_start = self.pos;
        let bytes = self.source.as_bytes();
        while self.pos < bytes.len() {
            let b = bytes[self.pos];
            if b == b'\\' && self.pos + 1 < bytes.len() {
                self.pos += 2;
                continue;
            }
            if b == b'"' {
                let body: SmolStr = self.source[body_start..self.pos].into();
                self.pos += 1;
                let span = Span::anonymous(start..self.pos);
                // Try character-range pattern: 3 chars where the middle is `-`.
                let chars: Vec<char> = body.chars().collect();
                if chars.len() == 3 && chars[1] == '-' {
                    let from: SmolStr = chars[0].encode_utf8(&mut [0u8; 4]).into();
                    let to: SmolStr = chars[2].encode_utf8(&mut [0u8; 4]).into();
                    self.emit(Token::CharacterRange(from, to), span);
                } else {
                    self.emit(Token::QuotedLiteral(body), span);
                }
                return;
            }
            if b == b'\n' {
                break;
            }
            self.pos += 1;
        }
        self.errors.push(LexError {
            span: Span::anonymous(start..self.pos),
            message: "unterminated quoted literal".to_string(),
        });
    }

    fn lex_curly(&mut self) {
        let start = self.pos;
        debug_assert_eq!(self.peek_byte(), Some(b'{'));
        self.pos += 1;
        let body_start = self.pos;
        let bytes = self.source.as_bytes();
        while self.pos < bytes.len() {
            let b = bytes[self.pos];
            if b == b'\\' && self.pos + 1 < bytes.len() {
                self.pos += 2;
                continue;
            }
            if b == b'}' {
                let body: SmolStr = self.source[body_start..self.pos].into();
                self.pos += 1;
                self.emit(Token::CurlyLiteral(body), Span::anonymous(start..self.pos));
                return;
            }
            self.pos += 1;
        }
        self.errors.push(LexError {
            span: Span::anonymous(start..self.pos),
            message: "unterminated curly literal".to_string(),
        });
    }

    // ───────────────────────── variable-name keywords ─────────────────────────

    fn try_variable_name_keyword(&mut self) -> Option<()> {
        for kw in VARIABLE_NAMES {
            if self.match_word(kw) {
                let start = self.pos;
                self.pos += kw.len();
                let span = Span::anonymous(start..self.pos);
                self.emit(Token::VariableName((*kw).into()), span);
                return Some(());
            }
        }
        None
    }

    // ───────────────────────── identifier ─────────────────────────

    fn lex_identifier_or_keyword(&mut self) {
        let start = self.pos;
        let mut text = SmolStrBuilder::new();
        let bytes = self.source.as_bytes();
        loop {
            if self.pos >= bytes.len() {
                break;
            }
            let b = bytes[self.pos];
            if b == b'%' {
                self.pos += 1;
                let rest = &self.source[self.pos..];
                if let Some(c) = rest.chars().next() {
                    text.push(c);
                    self.pos += c.len_utf8();
                    continue;
                }
                break;
            }
            if is_name_char_byte(b) {
                text.push(b as char);
                self.pos += 1;
                continue;
            }
            if b >= 0x80 {
                let rest = &self.source[self.pos..];
                if let Some(c) = rest.chars().next() {
                    text.push(c);
                    self.pos += c.len_utf8();
                    continue;
                }
            }
            break;
        }
        let text = text.finish();
        if text.is_empty() {
            self.error("expected an identifier");
            self.advance_one_char();
            return;
        }

        // Function-call form? Next byte = `(`.
        if self.peek_byte() == Some(b'(') {
            self.pos += 1;
            let span = Span::anonymous(start..self.pos);
            let tok = match text.as_str() {
                "Lit" => Token::LitLeft,
                "Ins" => Token::InsLeft,
                "EndTag" => Token::EndTagLeft,
                "Capture" => Token::CaptureLeft,
                "Cap" => Token::CapLeft,
                "OptCap" => Token::OptCapLeft,
                "DownCase" => Token::ToLowerLeft,
                "UpCase" => Token::ToUpperLeft,
                "OptDownCase" => Token::OptToLowerLeft,
                "OptUpCase" => Token::OptToUpperLeft,
                "AnyCase" => Token::AnyCaseLeft,
                "Explode" => Token::ExplodeLeft,
                "Implode" => Token::ImplodeLeft,
                "LC" => Token::LcLeft,
                "RC" => Token::RcLeft,
                "NLC" => Token::NlcLeft,
                "NRC" => Token::NrcLeft,
                "OR" => Token::OrLeft,
                "AND" => Token::AndLeft,
                "Lst" => Token::LstLeft,
                "Exc" => Token::ExcLeft,
                "Like" => Token::LikeLeft,
                "Unlike" => Token::UnlikeLeft,
                "Interpolate" => Token::InterpolateLeft,
                "Sigma" => Token::SigmaLeft,
                "Counter" => Token::CounterLeft,
                "Define" | "define" => Token::DefineLeft,
                "Uncompose" => Token::UncomposeLeft,
                _ => Token::SymbolWithLeftParen(text),
            };
            self.emit(tok, span);
            return;
        }

        // Bare-form keywords.
        let span = Span::anonymous(start..self.pos);
        let tok = match text.as_str() {
            "Define" | "define" | "DefFun" => Token::Define,
            "DefIns" => Token::DefIns,
            "regex" => Token::Regex,
            "set" => Token::SetVariable,
            "list" => Token::DefinedList,
            "Alpha" => Token::Alpha,
            "UppercaseAlpha" => Token::UppercaseAlpha,
            "LowercaseAlpha" => Token::LowercaseAlpha,
            "Num" => Token::Num,
            "Punct" => Token::Punct,
            "Whitespace" => Token::Whitespace,
            _ => Token::Symbol(text),
        };
        self.emit(tok, span);
    }
}

// ───────────────────────── helpers ─────────────────────────

fn is_name_char_byte(b: u8) -> bool {
    if !(0x21..=0x7e).contains(&b) {
        return b >= 0x80; // high-bit UTF-8 continues an identifier
    }
    !matches!(
        b,
        b'-' | b' '
            | b'|'
            | b'<'
            | b'>'
            | b'%'
            | b'^'
            | b':'
            | b';'
            | b','
            | b'@'
            | b'~'
            | b'\\'
            | b'&'
            | b'?'
            | b'$'
            | b'+'
            | b'*'
            | b'/'
            | b'('
            | b')'
            | b'{'
            | b'}'
            | b']'
            | b'['
    )
}

fn is_weight_start(b: Option<u8>) -> bool {
    matches!(b, Some(b'0'..=b'9') | Some(b'-') | Some(b'+'))
}

fn is_weight_byte(b: u8) -> bool {
    matches!(b, b'0'..=b'9' | b'-' | b'+' | b'.')
}

fn parse_uint(bytes: &[u8], at: usize) -> Option<(u32, usize)> {
    let mut i = at;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == at {
        return None;
    }
    let n: u32 = std::str::from_utf8(&bytes[at..i]).ok()?.parse().ok()?;
    Some((n, i))
}

fn build_read_bin(s: SmolStr) -> Token {
    Token::ReadBin(s)
}
fn build_read_text(s: SmolStr) -> Token {
    Token::ReadText(s)
}
fn build_read_spaced(s: SmolStr) -> Token {
    Token::ReadSpaced(s)
}
fn build_read_prolog(s: SmolStr) -> Token {
    Token::ReadProlog(s)
}
fn build_read_lexc(s: SmolStr) -> Token {
    Token::ReadLexc(s)
}
fn build_read_re(s: SmolStr) -> Token {
    Token::ReadRe(s)
}
fn build_read_vec(s: SmolStr) -> Token {
    Token::ReadVec(s)
}

// ───────────────────────── tests ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(src: &str) -> Vec<Token> {
        tokenize(src)
            .unwrap_or_else(|e| panic!("lex failed: {e:?}"))
            .into_iter()
            .map(|(t, _)| t)
            .collect()
    }

    #[test]
    fn empty() {
        assert!(lex("").is_empty());
    }

    #[test]
    fn smallest_define() {
        let toks = lex(r#"Define TOP "foo";"#);
        assert_eq!(toks[0], Token::Define);
        assert_eq!(toks[1], Token::Symbol("TOP".into()));
        assert_eq!(toks[2], Token::QuotedLiteral("foo".into()));
        assert_eq!(toks[3], Token::EndOfWeightedExpression(0.0));
    }

    #[test]
    fn lit_function_call() {
        let toks = lex("Lit(name)");
        assert_eq!(toks[0], Token::LitLeft);
        assert_eq!(toks[1], Token::Symbol("name".into()));
        assert_eq!(toks[2], Token::RightParenthesis);
    }

    #[test]
    fn user_function_call() {
        let toks = lex("MyFunc(a)");
        assert_eq!(toks[0], Token::SymbolWithLeftParen("MyFunc".into()));
    }

    #[test]
    fn acceptors() {
        let toks = lex("Alpha Num Whitespace");
        assert_eq!(toks, vec![Token::Alpha, Token::Num, Token::Whitespace]);
    }

    #[test]
    fn pair_separator_variants() {
        // four cases: " : ", " :", ": ", ":"
        let toks = lex("a : b");
        assert!(matches!(toks[1], Token::PairSeparatorSole));

        let toks = lex("a :b");
        assert!(matches!(toks[1], Token::PairSeparatorWoLeft));

        let toks = lex("a: b");
        assert!(matches!(toks[1], Token::PairSeparatorWoRight));

        let toks = lex("a:b");
        assert!(matches!(toks[1], Token::PairSeparator));
    }

    #[test]
    fn variable_name_with_dash() {
        let toks = lex("set need-separators off");
        assert_eq!(toks[0], Token::SetVariable);
        assert_eq!(toks[1], Token::VariableName("need-separators".into()));
        assert_eq!(toks[2], Token::Symbol("off".into()));
    }

    #[test]
    fn weight_on_semicolon() {
        let toks = lex("a ;::1.5");
        assert_eq!(toks[0], Token::Symbol("a".into()));
        assert_eq!(toks[1], Token::EndOfWeightedExpression(1.5));
    }

    #[test]
    fn character_range() {
        let toks = lex(r#""a-z""#);
        assert_eq!(toks[0], Token::CharacterRange("a".into(), "z".into()));
    }

    #[test]
    fn at_forms() {
        let toks = lex(r#"@bin"x.bin" @"y.fst" @vec"v""#);
        assert_eq!(toks[0], Token::ReadBin("x.bin".into()));
        assert_eq!(toks[1], Token::ReadBin("y.fst".into()));
        assert_eq!(toks[2], Token::ReadVec("v".into()));
    }

    #[test]
    fn endtag_function_call() {
        let toks = lex("EndTag(W)");
        assert_eq!(toks[0], Token::EndTagLeft);
    }

    #[test]
    fn catenation_n_forms() {
        let toks = lex("a^3 b^>2 c^<5 d^2,7");
        assert!(matches!(toks[1], Token::CatenateN(3)));
        assert!(matches!(toks[3], Token::CatenateNPlus(2)));
        assert!(matches!(toks[5], Token::CatenateNMinus(5)));
        assert!(matches!(toks[7], Token::CatenateNToK(2, 7)));
    }

    #[test]
    fn comment_skipped() {
        let toks = lex("! comment line\nDefine TOP a;");
        assert_eq!(toks[0], Token::Define);
    }

    #[test]
    fn replace_arrows() {
        use Token::*;
        let toks = lex("-> <- <-> (->) (<-) (<->) @-> ->@ @> >@");
        assert_eq!(
            toks,
            vec![
                ReplaceRight,
                ReplaceLeft,
                ReplaceLeftRight,
                OptionalReplaceRight,
                OptionalReplaceLeft,
                OptionalReplaceLeftRight,
                LtrLongestMatch,
                RtlLongestMatch,
                LtrShortestMatch,
                RtlShortestMatch,
            ]
        );
    }

    #[test]
    fn dot_keywords_priority() {
        // `.t(` should be TagLeft, not `.t` + `(`.
        let toks = lex(".t(x)");
        assert_eq!(toks[0], Token::TagLeft);
        assert_eq!(toks[1], Token::Symbol("x".into()));
        assert_eq!(toks[2], Token::RightParenthesis);
    }

    #[test]
    fn boundary_marker_in_dot_form() {
        let toks = lex(".#.");
        assert_eq!(toks[0], Token::BoundaryMarker);
    }
}
