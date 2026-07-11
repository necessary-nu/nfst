//! Hand-rolled twolc lexer. One token enum, no flex-style states; the
//! upstream pre1/pre2/pre3 staging is collapsed.
//!
//! NAME_CH set: any byte not in the upstream RESERVED_SYMBOL set
//! `*+/\\="$?|&^\-{}[]():;_!%~`. So digits, `.`, `,`, `<`, `>`, `@`, `#`,
//! `'`, letters, and any non-ASCII codepoint are valid in symbols. `%`
//! is the escape prefix — `%X` decodes to `X`.

use crate::token::Token;
use nfst_syntax::Span;
use smol_str::SmolStrBuilder;

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

struct Lexer<'a> {
    source: &'a str,
    pos: usize,
    tokens: Vec<(Token, Span)>,
    errors: Vec<LexError>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
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
                self.errors.push(LexError {
                    span: Span::anonymous(self.pos..self.pos + 1),
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

    // ───────────────────────── dispatcher ─────────────────────────

    fn step(&mut self) {
        // Section keywords (must be word-bounded). Tested first so they
        // win over generic identifier scanning that would also match
        // e.g. `Alphabet` as a Symbol.
        for (kw, tok) in [
            ("Alphabet", Token::SectionAlphabet),
            ("Diacritics", Token::SectionDiacritics),
            ("Sets", Token::SectionSets),
            ("Definitions", Token::SectionDefinitions),
            ("Rules", Token::SectionRules),
        ] {
            if self.match_word(kw) {
                let start = self.pos;
                self.pos += kw.len();
                self.emit(tok, Span::anonymous(start..self.pos));
                return;
            }
        }
        for (kw, tok) in [
            ("where", Token::Where),
            ("except", Token::Except),
            ("matched", Token::Matched),
            ("mixed", Token::Mixed),
            ("freely", Token::Freely),
            ("in", Token::In),
            ("and", Token::And),
        ] {
            if self.match_word(kw) {
                let start = self.pos;
                self.pos += kw.len();
                self.emit(tok, Span::anonymous(start..self.pos));
                return;
            }
        }

        // Long operators (longest first within each prefix).
        // 4-char arrows
        if let Some(span) = self.consume("/<==") {
            self.emit(Token::ReLeftRestrictionArrow, span);
            return;
        }
        if let Some(span) = self.consume("<==>") {
            self.emit(Token::ReLeftRightArrow, span);
            return;
        }
        // 3-char arrows
        if let Some(span) = self.consume("/<=") {
            self.emit(Token::LeftRestrictionArrow, span);
            return;
        }
        if let Some(span) = self.consume("<==") {
            self.emit(Token::ReLeftArrow, span);
            return;
        }
        if let Some(span) = self.consume("==>") {
            self.emit(Token::ReRightArrow, span);
            return;
        }
        if let Some(span) = self.consume("<=>") {
            self.emit(Token::LeftRightArrow, span);
            return;
        }
        // 2-char arrows / brackets
        if let Some(span) = self.consume("<=") {
            self.emit(Token::LeftArrow, span);
            return;
        }
        if let Some(span) = self.consume("=>") {
            self.emit(Token::RightArrow, span);
            return;
        }
        if let Some(span) = self.consume("<[") {
            self.emit(Token::ReLeftBracket, span);
            return;
        }
        if let Some(span) = self.consume("]>") {
            self.emit(Token::ReRightBracket, span);
            return;
        }
        if let Some(span) = self.consume("$.") {
            self.emit(Token::ContainmentOnce, span);
            return;
        }

        // `:` gets special handling: when followed by anything other than
        // a name-starter (`%` escape or `?`), upstream pre1 emits an
        // implicit `?` after the colon — `name:` with whitespace after is
        // shorthand for `name:?` (wildcard lower side).
        if self.peek_byte() == Some(b':') {
            let start = self.pos;
            self.pos += 1;
            self.emit(Token::Colon, Span::anonymous(start..self.pos));
            let next = self.peek_byte();
            let needs_implicit_q = match next {
                None => true,
                Some(b'%') => false,
                Some(b'?') => false,
                Some(b) if is_name_char_byte(b) => false,
                Some(_) => true,
            };
            if needs_implicit_q {
                self.emit(Token::QuestionMark, Span::anonymous(self.pos..self.pos));
            }
            return;
        }

        // Single-char operators.
        if let Some(b) = self.peek_byte() {
            let tok = match b {
                b'*' => Some(Token::Star),
                b'+' => Some(Token::Plus),
                b'/' => Some(Token::FreelyInsert),
                b'~' => Some(Token::Complement),
                b'\\' => Some(Token::TermComplement),
                b'$' => Some(Token::Containment),
                b'|' => Some(Token::Union),
                b'&' => Some(Token::Intersection),
                b'-' => Some(Token::Difference),
                b'^' => Some(Token::Power),
                b'?' => Some(Token::QuestionMark),
                b'[' => Some(Token::LeftBracket),
                b']' => Some(Token::RightBracket),
                b'(' => Some(Token::LeftParenthesis),
                b')' => Some(Token::RightParenthesis),
                b'{' => Some(Token::LeftCurly),
                b'}' => Some(Token::RightCurly),
                b';' => Some(Token::Semicolon),
                b'=' => Some(Token::Equals),
                b'_' => Some(Token::CenterMarker),
                b',' => Some(Token::Comma),
                _ => None,
            };
            if let Some(tok) = tok {
                let start = self.pos;
                self.pos += 1;
                self.emit(tok, Span::anonymous(start..self.pos));
                return;
            }
        }

        // Quoted rule name.
        if self.peek_byte() == Some(b'"') {
            self.lex_rule_name();
            return;
        }

        // Identifier (FREE_SYMBOL+, with `%X` escapes). Digits are
        // included as Symbol content; the parser converts when it sees a
        // count after `^`.
        self.lex_symbol();
    }

    fn lex_rule_name(&mut self) {
        let start = self.pos;
        debug_assert_eq!(self.peek_byte(), Some(b'"'));
        self.pos += 1;
        let body_start = self.pos;
        let bytes = self.source.as_bytes();
        while self.pos < bytes.len() {
            let b = bytes[self.pos];
            if b == b'"' {
                let body = &self.source[body_start..self.pos];
                self.pos += 1;
                self.emit(
                    Token::RuleName(body.into()),
                    Span::anonymous(start..self.pos),
                );
                return;
            }
            if b == b'\n' {
                break;
            }
            self.pos += 1;
        }
        self.errors.push(LexError {
            span: Span::anonymous(start..self.pos),
            message: "unterminated rule name".to_string(),
        });
    }

    fn lex_symbol(&mut self) {
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
                let rest = self.rest();
                if let Some(c) = rest.chars().next() {
                    text.push(c);
                    self.pos += c.len_utf8();
                    continue;
                }
                break;
            }
            // High-bit bytes are UTF-8 continuation; decode the full
            // codepoint before pushing.  Pushing the raw byte as `char`
            // would Latin-1-ify it.
            if b >= 0x80 {
                let rest = self.rest();
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
            break;
        }
        let text = text.finish();
        if text.is_empty() {
            self.errors.push(LexError {
                span: Span::anonymous(start..start + 1),
                message: format!("unexpected byte {:?}", bytes.get(start)),
            });
            self.advance_one_char();
            return;
        }
        self.emit(Token::Symbol(text), Span::anonymous(start..self.pos));
    }
}

/// True if a byte is a NAME_CH continuation (the upstream FREE_SYMBOL
/// set: anything outside the reserved set, plus high-bit UTF-8 bytes).
fn is_name_char_byte(b: u8) -> bool {
    if b >= 0x80 {
        return true;
    }
    if b < 0x21 {
        return false; // whitespace and control chars
    }
    !matches!(
        b,
        b'*' | b'+'
            | b'/'
            | b'\\'
            | b'='
            | b'"'
            | b'$'
            | b'?'
            | b'|'
            | b'&'
            | b'^'
            | b'-'
            | b'{'
            | b'}'
            | b'['
            | b']'
            | b'('
            | b')'
            | b':'
            | b';'
            | b'_'
            | b'!'
            | b'%'
            | b'~'
    )
}

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
    fn empty_input() {
        assert!(lex("").is_empty());
    }

    #[test]
    fn smallest_grammar() {
        let toks = lex("Alphabet a b ;\nRules\n\"r\"\na:b => b _ b ;");
        assert_eq!(toks[0], Token::SectionAlphabet);
        assert_eq!(toks[1], Token::Symbol("a".into()));
        assert_eq!(toks[2], Token::Symbol("b".into()));
        assert_eq!(toks[3], Token::Semicolon);
        assert_eq!(toks[4], Token::SectionRules);
        assert_eq!(toks[5], Token::RuleName("r".into()));
        assert_eq!(toks[6], Token::Symbol("a".into()));
        assert_eq!(toks[7], Token::Colon);
        assert_eq!(toks[8], Token::Symbol("b".into()));
        assert_eq!(toks[9], Token::RightArrow);
        assert_eq!(toks[10], Token::Symbol("b".into()));
        assert_eq!(toks[11], Token::CenterMarker);
        assert_eq!(toks[12], Token::Symbol("b".into()));
        assert_eq!(toks[13], Token::Semicolon);
    }

    #[test]
    fn all_rule_arrows() {
        let toks = lex("<= => <=> /<=");
        assert_eq!(toks[0], Token::LeftArrow);
        assert_eq!(toks[1], Token::RightArrow);
        assert_eq!(toks[2], Token::LeftRightArrow);
        assert_eq!(toks[3], Token::LeftRestrictionArrow);
    }

    #[test]
    fn re_arrows() {
        let toks = lex("<== ==> <==> /<==");
        assert_eq!(toks[0], Token::ReLeftArrow);
        assert_eq!(toks[1], Token::ReRightArrow);
        assert_eq!(toks[2], Token::ReLeftRightArrow);
        assert_eq!(toks[3], Token::ReLeftRestrictionArrow);
    }

    #[test]
    fn keywords() {
        let toks = lex("where except matched mixed freely in and");
        assert_eq!(
            toks,
            vec![
                Token::Where,
                Token::Except,
                Token::Matched,
                Token::Mixed,
                Token::Freely,
                Token::In,
                Token::And,
            ]
        );
    }

    #[test]
    fn comments_and_whitespace() {
        let toks = lex("Alphabet ! comment\n a ;\n");
        assert_eq!(toks[0], Token::SectionAlphabet);
        assert_eq!(toks[1], Token::Symbol("a".into()));
        assert_eq!(toks[2], Token::Semicolon);
    }

    #[test]
    fn percent_escapes() {
        // `%>` → `>` in symbol
        let toks = lex("%>");
        assert_eq!(toks[0], Token::Symbol(">".into()));
    }

    #[test]
    fn number_after_power_is_symbol() {
        // Digits are Symbol content; the parser interprets them as counts.
        let toks = lex("^3 ^3,5");
        assert_eq!(toks[0], Token::Power);
        assert_eq!(toks[1], Token::Symbol("3".into()));
        assert_eq!(toks[2], Token::Power);
        assert_eq!(toks[3], Token::Symbol("3,5".into()));
    }

    #[test]
    fn rule_name_quotes() {
        let toks = lex(r#""my rule""#);
        assert_eq!(toks[0], Token::RuleName("my rule".into()));
    }

    #[test]
    fn pair_with_zero_for_epsilon() {
        // `e:0` — `0` is just a Symbol containing "0".
        let toks = lex("e:0");
        assert_eq!(toks[0], Token::Symbol("e".into()));
        assert_eq!(toks[1], Token::Colon);
        assert_eq!(toks[2], Token::Symbol("0".into()));
    }

    #[test]
    fn symbol_with_dot() {
        let toks = lex(".#.");
        assert_eq!(toks[0], Token::Symbol(".#.".into()));
    }

    #[test]
    fn re_brackets() {
        let toks = lex("<[ a ]>");
        assert_eq!(toks[0], Token::ReLeftBracket);
        assert_eq!(toks[1], Token::Symbol("a".into()));
        assert_eq!(toks[2], Token::ReRightBracket);
    }

    #[test]
    fn multichar_with_digits() {
        // `e9` — a multi-char symbol that includes a digit
        let toks = lex("e9");
        assert_eq!(toks[0], Token::Symbol("e9".into()));
    }
}
