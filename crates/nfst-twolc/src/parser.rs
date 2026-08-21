//! Recursive-descent twolc parser. Single-pass; no flex-style staging.
//!
//! Top-level: five optional sections in order (Alphabet, Diacritics,
//! Sets, Definitions, Rules). The Rules section is mandatory in upstream
//! and gets the same treatment here.
//!
//! Twolc's regex sublanguage is smaller than xre's — no replace arrows,
//! no `@`-files, no markup. We parse it in 4 precedence layers:
//! union/intersection/difference (lowest), concatenation, postfix
//! repetition (`*`, `+`, `^N`), prefix unaries (`~`, `\`, `$`, `$.`),
//! atoms.

use crate::ast::{
    AlphabetPair, BinaryOp, RuleCenter, RuleContext, RuleOp, SetDefinition, TwolcDefinition,
    TwolcFile, TwolcRegex, TwolcRule, UnaryOp, VarMatcher, VariableAssignment, VariableBlock,
};
use crate::lexer::{LexError, tokenize};
use crate::token::{Token, describe};
use nfst_syntax::{Diagnostic, Span, Spanned};
use smol_str::SmolStr;

#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse(source: &str) -> Result<Spanned<TwolcFile>, ParseError> {
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

    fn expect(&mut self, expected: &Token, label: &str) -> Result<Span, Diagnostic> {
        match self.peek() {
            Some(t) if std::mem::discriminant(t) == std::mem::discriminant(expected) => {
                Ok(self.bump().unwrap().1)
            }
            _ => Err(self.err(format!("expected {label}, got {}", describe(self.peek())))),
        }
    }

    fn spanned(value: TwolcRegex, span: Span) -> Spanned<TwolcRegex> {
        Spanned::new(value, span)
    }

    // ───────────────────────── top level ─────────────────────────

    fn parse_file(&mut self) -> Result<Spanned<TwolcFile>, Diagnostic> {
        let start = self.current_start();
        let alphabet = self.try_parse_alphabet()?;
        let diacritics = self.try_parse_diacritics()?;
        let sets = self.try_parse_sets()?;
        let definitions = self.try_parse_definitions()?;
        let rules = self.parse_rules()?;

        if !self.is_at_end() {
            return Err(self.err(format!(
                "unexpected trailing input: {}",
                describe(self.peek())
            )));
        }

        Ok(Spanned::new(
            TwolcFile {
                alphabet,
                diacritics,
                sets,
                definitions,
                rules,
            },
            self.merge(start),
        ))
    }

    fn try_parse_alphabet(&mut self) -> Result<Vec<Spanned<AlphabetPair>>, Diagnostic> {
        if !matches!(self.peek(), Some(Token::SectionAlphabet)) {
            return Ok(Vec::new());
        }
        self.bump();
        let mut pairs = Vec::new();
        while !self.section_terminator_ahead() {
            let p = self.parse_alphabet_pair()?;
            pairs.push(p);
            // Optional `;` between pairs (multiple semicolons collapse).
            while matches!(self.peek(), Some(Token::Semicolon)) {
                self.bump();
            }
        }
        Ok(pairs)
    }

    fn parse_alphabet_pair(&mut self) -> Result<Spanned<AlphabetPair>, Diagnostic> {
        let start = self.current_start();
        let upper = self.expect_symbol_string("alphabet pair upper")?;
        let lower = if matches!(self.peek(), Some(Token::Colon)) {
            self.bump();
            self.expect_symbol_string("alphabet pair lower")?
        } else {
            // Single-symbol form: `a` is identity-encoded as `a:a`.
            upper.clone()
        };
        Ok(Spanned::new(
            AlphabetPair { upper, lower },
            self.merge(start),
        ))
    }

    fn try_parse_diacritics(&mut self) -> Result<Vec<Spanned<SmolStr>>, Diagnostic> {
        if !matches!(self.peek(), Some(Token::SectionDiacritics)) {
            return Ok(Vec::new());
        }
        self.bump();
        let mut out = Vec::new();
        while !self.section_terminator_ahead() {
            let start = self.current_start();
            let s = self.expect_symbol_string("diacritic symbol")?;
            out.push(Spanned::new(s, self.merge(start)));
            while matches!(self.peek(), Some(Token::Semicolon)) {
                self.bump();
            }
        }
        Ok(out)
    }

    fn try_parse_sets(&mut self) -> Result<Vec<Spanned<SetDefinition>>, Diagnostic> {
        if !matches!(self.peek(), Some(Token::SectionSets)) {
            return Ok(Vec::new());
        }
        self.bump();
        let mut out = Vec::new();
        while !self.section_terminator_ahead() {
            let start = self.current_start();
            let name = self.expect_symbol_string("set name")?;
            self.expect(&Token::Equals, "`=`")?;
            let mut members = Vec::new();
            while !matches!(self.peek(), Some(Token::Semicolon)) {
                if self.section_terminator_ahead() {
                    break;
                }
                members.push(self.expect_symbol_string("set member")?);
            }
            self.expect(&Token::Semicolon, "`;`")?;
            out.push(Spanned::new(
                SetDefinition { name, members },
                self.merge(start),
            ));
        }
        Ok(out)
    }

    fn try_parse_definitions(&mut self) -> Result<Vec<Spanned<TwolcDefinition>>, Diagnostic> {
        if !matches!(self.peek(), Some(Token::SectionDefinitions)) {
            return Ok(Vec::new());
        }
        self.bump();
        let mut out = Vec::new();
        while !self.section_terminator_ahead() {
            let start = self.current_start();
            let name = self.expect_symbol_string("definition name")?;
            self.expect(&Token::Equals, "`=`")?;
            let body = self.parse_regex(/*stop_on_semicolon=*/ true)?;
            self.expect(&Token::Semicolon, "`;`")?;
            out.push(Spanned::new(
                TwolcDefinition { name, body },
                self.merge(start),
            ));
        }
        Ok(out)
    }

    fn parse_rules(&mut self) -> Result<Vec<Spanned<TwolcRule>>, Diagnostic> {
        match self.peek() {
            Some(Token::SectionRules) => {
                self.bump();
            }
            _ => {
                return Err(self.err(format!(
                    "expected `Rules` section, got {}",
                    describe(self.peek())
                )));
            }
        }
        let mut rules = Vec::new();
        while !self.is_at_end() {
            rules.push(self.parse_rule()?);
        }
        Ok(rules)
    }

    fn parse_rule(&mut self) -> Result<Spanned<TwolcRule>, Diagnostic> {
        let start = self.current_start();
        let name = match self.peek().cloned() {
            Some(Token::RuleName(s)) => {
                self.bump();
                s
            }
            other => {
                return Err(self.err(format!(
                    "expected rule name (`\"…\"`), got {}",
                    describe(other.as_ref())
                )));
            }
        };
        let center = self.parse_rule_center()?;
        let operator = self.parse_rule_operator()?;
        let positive_contexts = self.parse_rule_contexts()?;
        let negative_contexts = if matches!(self.peek(), Some(Token::Except)) {
            self.bump();
            self.parse_rule_contexts()?
        } else {
            Vec::new()
        };
        let variables = if matches!(self.peek(), Some(Token::Where)) {
            Some(self.parse_where_clause()?)
        } else {
            None
        };
        Ok(Spanned::new(
            TwolcRule {
                name,
                center,
                operator,
                positive_contexts,
                negative_contexts,
                variables,
            },
            self.merge(start),
        ))
    }

    fn parse_rule_center(&mut self) -> Result<RuleCenter, Diagnostic> {
        // RE-form rule center: `<[ E ]>`
        if matches!(self.peek(), Some(Token::ReLeftBracket)) {
            self.bump();
            let body = self.parse_regex(/*stop_on_semicolon=*/ false)?;
            self.expect(&Token::ReRightBracket, "`]>`")?;
            return Ok(RuleCenter::Regex(Box::new(body)));
        }
        // Bracketed list of pairs: `[ a:b | c:d ]`
        if matches!(self.peek(), Some(Token::LeftBracket)) {
            self.bump();
            let pairs = self.parse_pair_list()?;
            self.expect(&Token::RightBracket, "`]`")?;
            return Ok(RuleCenter::Pair(pairs));
        }
        let pairs = self.parse_pair_list()?;
        Ok(RuleCenter::Pair(pairs))
    }

    fn parse_pair_list(&mut self) -> Result<Vec<AlphabetPair>, Diagnostic> {
        let mut pairs = Vec::new();
        pairs.push(self.parse_pair_only()?);
        while matches!(self.peek(), Some(Token::Union)) {
            self.bump();
            pairs.push(self.parse_pair_only()?);
        }
        Ok(pairs)
    }

    /// Upstream pre1's `PAIR` productions (htwolcpre1-parser.yy): an elided
    /// side defaults rather than being an error, so `X` is the identity pair
    /// `X:X`, `X:` is `X:?`, `:Y` is `?:Y` and a lone `:` is `?:?`. Only
    /// `X:Y` writes both sides out. The bare form is why a rule centre can
    /// be a single symbol — `%{hyph%?%} <= _ ;`.
    fn parse_pair_only(&mut self) -> Result<AlphabetPair, Diagnostic> {
        // `:Y` and `:` — no upper side written, so the upper is the
        // wildcard. The lexer already supplies the `?` for a `:` that no
        // symbol follows, so the lower side reads as an ordinary symbol.
        if matches!(self.peek(), Some(Token::Colon)) {
            self.bump();
            let lower = self.expect_pair_symbol("pair lower")?;
            return Ok(AlphabetPair {
                upper: "?".into(),
                lower,
            });
        }
        let upper = self.expect_pair_symbol("pair upper")?;
        if !matches!(self.peek(), Some(Token::Colon)) {
            return Ok(AlphabetPair {
                lower: upper.clone(),
                upper,
            });
        }
        self.bump();
        let lower = self.expect_pair_symbol("pair lower")?;
        Ok(AlphabetPair { upper, lower })
    }

    fn expect_pair_symbol(&mut self, label: &str) -> Result<SmolStr, Diagnostic> {
        match self.peek().cloned() {
            Some(Token::Symbol(s)) => {
                self.bump();
                Ok(s)
            }
            Some(Token::QuestionMark) => {
                self.bump();
                Ok("?".into())
            }
            other => Err(self.err(format!(
                "expected {label} symbol, got {}",
                describe(other.as_ref())
            ))),
        }
    }

    fn parse_rule_operator(&mut self) -> Result<RuleOp, Diagnostic> {
        let op = match self.peek() {
            Some(Token::RightArrow) | Some(Token::ReRightArrow) => RuleOp::Right,
            Some(Token::LeftArrow) | Some(Token::ReLeftArrow) => RuleOp::Left,
            Some(Token::LeftRightArrow) | Some(Token::ReLeftRightArrow) => RuleOp::LeftRight,
            Some(Token::LeftRestrictionArrow) | Some(Token::ReLeftRestrictionArrow) => {
                RuleOp::NotLeft
            }
            other => {
                return Err(self.err(format!(
                    "expected rule arrow (`=>`, `<=`, `<=>`, `/<=`), got {}",
                    describe(other)
                )));
            }
        };
        self.bump();
        Ok(op)
    }

    fn parse_rule_contexts(&mut self) -> Result<Vec<RuleContext>, Diagnostic> {
        let mut contexts = Vec::new();
        while !self.context_list_ended() {
            let left = self.parse_regex(/*stop_on_semicolon=*/ false)?;
            self.expect(&Token::CenterMarker, "`_`")?;
            let right = self.parse_regex(/*stop_on_semicolon=*/ true)?;
            self.expect(&Token::Semicolon, "`;`")?;
            // Multiple `;` collapse.
            while matches!(self.peek(), Some(Token::Semicolon)) {
                self.bump();
            }
            contexts.push(RuleContext { left, right });
        }
        Ok(contexts)
    }

    fn context_list_ended(&self) -> bool {
        matches!(
            self.peek(),
            None | Some(Token::RuleName(_))
                | Some(Token::Except)
                | Some(Token::Where)
                | Some(Token::SectionRules)
        )
    }

    fn parse_where_clause(&mut self) -> Result<Vec<VariableBlock>, Diagnostic> {
        self.expect(&Token::Where, "`where`")?;
        let mut blocks = Vec::new();
        blocks.push(self.parse_variable_block()?);
        while matches!(self.peek(), Some(Token::And)) {
            self.bump();
            blocks.push(self.parse_variable_block()?);
        }
        // Trailing `;`.
        while matches!(self.peek(), Some(Token::Semicolon)) {
            self.bump();
        }
        Ok(blocks)
    }

    fn parse_variable_block(&mut self) -> Result<VariableBlock, Diagnostic> {
        let mut assignments = Vec::new();
        loop {
            // A block ends at a matcher keyword, `and`, `;`, EOF, or any
            // token that wouldn't begin an assignment.
            if matches!(
                self.peek(),
                None | Some(Token::Matched)
                    | Some(Token::Mixed)
                    | Some(Token::Freely)
                    | Some(Token::And)
                    | Some(Token::Semicolon)
            ) {
                break;
            }
            // Otherwise: `name in ( v1 v2 … )`, or the unparenthesised
            // `name in v` form.
            let name = self.expect_symbol_string("variable name")?;
            self.expect(&Token::In, "`in`")?;
            let values = if matches!(self.peek(), Some(Token::LeftParenthesis)) {
                self.bump();
                let mut values = Vec::new();
                while !matches!(self.peek(), Some(Token::RightParenthesis)) {
                    values.push(self.expect_symbol_string("variable value")?);
                }
                self.expect(&Token::RightParenthesis, "`)`")?;
                values
            } else {
                // Upstream's second production, `VAR_SYMBOL IN VAR_SYMBOL`:
                // exactly one symbol, in practice the name of a Set. Both
                // productions funnel through the same `set_variable_values`,
                // which expands set names and passes anything else through as
                // a literal — so a bare name means the same as a one-element
                // parenthesised list, and resolution stays downstream. Taking
                // exactly one symbol is what keeps `in a b c` an error, as it
                // is upstream.
                vec![self.expect_symbol_string("variable value")?]
            };
            assignments.push(VariableAssignment { name, values });
            // Block continues until `matched`/`mixed`/`freely` or `and`/EOL.
            if matches!(self.peek(), Some(Token::And)) || self.is_at_end() {
                break;
            }
        }
        let matcher = match self.peek() {
            Some(Token::Matched) => {
                self.bump();
                VarMatcher::Matched
            }
            Some(Token::Mixed) => {
                self.bump();
                VarMatcher::Mixed
            }
            Some(Token::Freely) => {
                self.bump();
                VarMatcher::Freely
            }
            // Default per upstream pre1: FREELY.
            _ => VarMatcher::Freely,
        };
        Ok(VariableBlock {
            assignments,
            matcher,
        })
    }

    // ───────────────────────── regex ─────────────────────────

    fn parse_regex(&mut self, stop_on_semicolon: bool) -> Result<Spanned<TwolcRegex>, Diagnostic> {
        let start = self.current_start();
        // Empty regex (e.g. `_ ;` left context, or `... _;` right context).
        if !self.peek_starts_atom(stop_on_semicolon) {
            return Ok(Self::spanned(TwolcRegex::Epsilon, self.merge(start)));
        }
        self.parse_regex_union(stop_on_semicolon)
    }

    fn parse_regex_union(&mut self, stop_on_semi: bool) -> Result<Spanned<TwolcRegex>, Diagnostic> {
        let start = self.current_start();
        let mut left = self.parse_regex_concat(stop_on_semi)?;
        loop {
            let op = match self.peek() {
                Some(Token::Union) => BinaryOp::Union,
                Some(Token::Intersection) => BinaryOp::Intersect,
                Some(Token::Difference) => BinaryOp::Subtract,
                Some(Token::FreelyInsert) => BinaryOp::Ignoring, // closest xre op
                _ => break,
            };
            self.bump();
            let right = self.parse_regex_concat(stop_on_semi)?;
            left = Self::spanned(
                TwolcRegex::Binary(op, Box::new(left), Box::new(right)),
                self.merge(start),
            );
        }
        Ok(left)
    }

    fn parse_regex_concat(
        &mut self,
        stop_on_semi: bool,
    ) -> Result<Spanned<TwolcRegex>, Diagnostic> {
        let start = self.current_start();
        let mut left = self.parse_regex_postfix(stop_on_semi)?;
        while self.peek_starts_atom(stop_on_semi) {
            let right = self.parse_regex_postfix(stop_on_semi)?;
            left = Self::spanned(
                TwolcRegex::Binary(BinaryOp::Concatenate, Box::new(left), Box::new(right)),
                self.merge(start),
            );
        }
        Ok(left)
    }

    fn parse_regex_postfix(
        &mut self,
        stop_on_semi: bool,
    ) -> Result<Spanned<TwolcRegex>, Diagnostic> {
        let start = self.current_start();
        let mut expr = self.parse_regex_prefix(stop_on_semi)?;
        loop {
            expr = match self.peek() {
                Some(Token::Star) => {
                    self.bump();
                    Self::spanned(
                        TwolcRegex::Unary(UnaryOp::Star, Box::new(expr)),
                        self.merge(start),
                    )
                }
                Some(Token::Plus) => {
                    self.bump();
                    Self::spanned(
                        TwolcRegex::Unary(UnaryOp::Plus, Box::new(expr)),
                        self.merge(start),
                    )
                }
                Some(Token::Power) => {
                    self.bump();
                    // After `^`, the next Symbol token's content is parsed
                    // as either `N` or `N,K`.
                    let (n, k) = self.parse_repeat_count()?;
                    let span = self.merge(start);
                    let value = match k {
                        None => TwolcRegex::RepeatN(Box::new(expr), n),
                        Some(k) => TwolcRegex::RepeatNToK(Box::new(expr), n, k),
                    };
                    Self::spanned(value, span)
                }
                _ => break,
            };
        }
        Ok(expr)
    }

    fn parse_repeat_count(&mut self) -> Result<(u32, Option<u32>), Diagnostic> {
        let Some(Token::Symbol(s)) = self.peek().cloned() else {
            return Err(self.err(format!(
                "expected number after `^`, got {}",
                describe(self.peek())
            )));
        };
        let span = self.peek_span();
        self.bump();
        if let Some((a, b)) = s.split_once(',') {
            let n: u32 = a
                .parse()
                .map_err(|_| Diagnostic::error(span.clone(), format!("not a number: {a:?}")))?;
            let k: u32 = b
                .parse()
                .map_err(|_| Diagnostic::error(span.clone(), format!("not a number: {b:?}")))?;
            Ok((n, Some(k)))
        } else {
            let n: u32 = s
                .parse()
                .map_err(|_| Diagnostic::error(span, format!("not a number: {s:?}")))?;
            Ok((n, None))
        }
    }

    fn parse_regex_prefix(
        &mut self,
        stop_on_semi: bool,
    ) -> Result<Spanned<TwolcRegex>, Diagnostic> {
        let start = self.current_start();
        match self.peek() {
            Some(Token::Complement) => {
                self.bump();
                let inner = self.parse_regex_prefix(stop_on_semi)?;
                Ok(Self::spanned(
                    TwolcRegex::Unary(UnaryOp::Complement, Box::new(inner)),
                    self.merge(start),
                ))
            }
            Some(Token::TermComplement) => {
                self.bump();
                let inner = self.parse_regex_prefix(stop_on_semi)?;
                Ok(Self::spanned(
                    TwolcRegex::Unary(UnaryOp::TermComplement, Box::new(inner)),
                    self.merge(start),
                ))
            }
            Some(Token::Containment) => {
                self.bump();
                let inner = self.parse_regex_prefix(stop_on_semi)?;
                Ok(Self::spanned(
                    TwolcRegex::Unary(UnaryOp::Containment, Box::new(inner)),
                    self.merge(start),
                ))
            }
            Some(Token::ContainmentOnce) => {
                self.bump();
                let inner = self.parse_regex_prefix(stop_on_semi)?;
                Ok(Self::spanned(
                    TwolcRegex::Unary(UnaryOp::ContainmentOnce, Box::new(inner)),
                    self.merge(start),
                ))
            }
            _ => self.parse_regex_atom(stop_on_semi),
        }
    }

    fn parse_regex_atom(&mut self, stop_on_semi: bool) -> Result<Spanned<TwolcRegex>, Diagnostic> {
        let start = self.current_start();
        // Leading `:something` — implicit `?` on the upper side.
        if matches!(self.peek(), Some(Token::Colon)) {
            let upper = Self::spanned(TwolcRegex::Any, self.peek_span());
            return self.parse_pair_tail(upper, start);
        }
        match self.peek().cloned() {
            Some(Token::LeftBracket) => {
                self.bump();
                let inner = self.parse_regex(/*stop_on_semicolon=*/ false)?;
                self.expect(&Token::RightBracket, "`]`")?;
                Ok(Self::spanned(
                    TwolcRegex::Group(Box::new(inner)),
                    self.merge(start),
                ))
            }
            Some(Token::LeftCurly) => {
                self.bump();
                let inner = self.parse_regex(/*stop_on_semicolon=*/ false)?;
                self.expect(&Token::RightCurly, "`}`")?;
                Ok(Self::spanned(
                    TwolcRegex::Group(Box::new(inner)),
                    self.merge(start),
                ))
            }
            Some(Token::LeftParenthesis) => {
                self.bump();
                let inner = self.parse_regex(/*stop_on_semicolon=*/ false)?;
                self.expect(&Token::RightParenthesis, "`)`")?;
                Ok(Self::spanned(
                    TwolcRegex::Optional(Box::new(inner)),
                    self.merge(start),
                ))
            }
            Some(Token::Symbol(s)) => {
                self.bump();
                let upper = Self::spanned(TwolcRegex::Symbol(s), self.merge(start));
                self.parse_pair_tail(upper, start)
            }
            Some(Token::QuestionMark) => {
                self.bump();
                let any = Self::spanned(TwolcRegex::Any, self.merge(start));
                self.parse_pair_tail(any, start)
            }
            other => {
                let _ = stop_on_semi;
                Err(self.err(format!(
                    "unexpected token in regex: {}",
                    describe(other.as_ref())
                )))
            }
        }
    }

    fn parse_pair_tail(
        &mut self,
        upper: Spanned<TwolcRegex>,
        start: usize,
    ) -> Result<Spanned<TwolcRegex>, Diagnostic> {
        if matches!(self.peek(), Some(Token::Colon)) {
            self.bump();
            // Upstream pre1 emits `: ?` when `:` is followed by a non-name
            // character (whitespace, `;`, `]`, etc.). The shorthand
            // `name:` thus means `name:?` — pair with wildcard lower.
            let lower = match self.peek().cloned() {
                Some(Token::Symbol(s)) => {
                    self.bump();
                    Self::spanned(TwolcRegex::Symbol(s), self.merge(start))
                }
                Some(Token::QuestionMark) => {
                    self.bump();
                    Self::spanned(TwolcRegex::Any, self.merge(start))
                }
                _ => Self::spanned(TwolcRegex::Any, self.merge(start)),
            };
            return Ok(Self::spanned(
                TwolcRegex::Pair {
                    upper: Box::new(upper),
                    lower: Box::new(lower),
                },
                self.merge(start),
            ));
        }
        Ok(upper)
    }

    /// Set used by parse_regex_concat to decide if the next token can
    /// continue a concatenation chain.
    fn peek_starts_atom(&self, stop_on_semi: bool) -> bool {
        match self.peek() {
            Some(Token::Symbol(_))
            | Some(Token::QuestionMark)
            | Some(Token::LeftBracket)
            | Some(Token::LeftCurly)
            | Some(Token::LeftParenthesis)
            | Some(Token::Complement)
            | Some(Token::TermComplement)
            | Some(Token::Containment)
            | Some(Token::ContainmentOnce)
            // Leading `:foo` is `?:foo` — also a valid atom start.
            | Some(Token::Colon) => true,
            Some(Token::Semicolon) if !stop_on_semi => false,
            _ => false,
        }
    }

    // ───────────────────────── helpers ─────────────────────────

    fn expect_symbol_string(&mut self, label: &str) -> Result<SmolStr, Diagnostic> {
        match self.peek().cloned() {
            Some(Token::Symbol(s)) => {
                self.bump();
                Ok(s)
            }
            other => Err(self.err(format!(
                "expected {label}, got {}",
                describe(other.as_ref())
            ))),
        }
    }

    /// True when the next token is the start of a different section. Used
    /// to terminate inside-section repeated parses (alphabet, sets, etc.).
    fn section_terminator_ahead(&self) -> bool {
        matches!(
            self.peek(),
            None | Some(Token::SectionAlphabet)
                | Some(Token::SectionDiacritics)
                | Some(Token::SectionSets)
                | Some(Token::SectionDefinitions)
                | Some(Token::SectionRules)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(src: &str) -> TwolcFile {
        parse(src)
            .unwrap_or_else(|e| panic!("parse {src:?}: {e:?}"))
            .value
    }

    #[test]
    fn smallest_grammar() {
        let f = parsed("Alphabet a b ;\nRules\n\"r\"\na:b => b _ b ;");
        assert_eq!(f.alphabet.len(), 2);
        assert_eq!(f.alphabet[0].value.upper, "a");
        assert_eq!(f.rules.len(), 1);
        assert_eq!(f.rules[0].value.name, "r");
        assert_eq!(f.rules[0].value.operator, RuleOp::Right);
    }

    #[test]
    fn all_arrows() {
        let f = parsed(
            "Alphabet a b c d ;\nRules\n\
             \"r1\" a:b => _ ;\n\
             \"r2\" a:b <= _ ;\n\
             \"r3\" a:b <=> _ ;\n\
             \"r4\" a:b /<= _ ;\n",
        );
        assert_eq!(f.rules.len(), 4);
        assert_eq!(f.rules[0].value.operator, RuleOp::Right);
        assert_eq!(f.rules[1].value.operator, RuleOp::Left);
        assert_eq!(f.rules[2].value.operator, RuleOp::LeftRight);
        assert_eq!(f.rules[3].value.operator, RuleOp::NotLeft);
    }

    fn center(f: &TwolcFile) -> &[AlphabetPair] {
        match &f.rules[0].value.center {
            RuleCenter::Pair(pairs) => pairs,
            other => panic!("expected a pair center, got {other:?}"),
        }
    }

    fn pair(upper: &str, lower: &str) -> AlphabetPair {
        AlphabetPair {
            upper: upper.into(),
            lower: lower.into(),
        }
    }

    #[test]
    fn bare_symbol_center_is_the_identity_pair() {
        // Upstream pre1's `PAIR: GRAMMAR_SYMBOL_SPACE` production.
        let f = parsed("Alphabet a b ;\nRules\n\"r\" a <= _ ;");
        assert_eq!(center(&f), [pair("a", "a")]);
    }

    #[test]
    fn bare_escaped_symbol_center() {
        // The omorfi shape: a multi-character escaped symbol standing alone
        // as the centre. `%{hyph%?%}` unescapes to `{hyph?}`.
        let f = parsed("Alphabet %{hyph%?%} ;\nRules\n\"r\" %{hyph%?%} <= _ ;");
        assert_eq!(center(&f), [pair("{hyph?}", "{hyph?}")]);
    }

    #[test]
    fn elided_center_sides_default_to_the_wildcard() {
        let f = parsed("Alphabet a b ;\nRules\n\"r\" a: <= _ ;");
        assert_eq!(center(&f), [pair("a", "?")]);

        let f = parsed("Alphabet a b ;\nRules\n\"r\" :b <= _ ;");
        assert_eq!(center(&f), [pair("?", "b")]);

        let f = parsed("Alphabet a b ;\nRules\n\"r\" : <= _ ;");
        assert_eq!(center(&f), [pair("?", "?")]);
    }

    #[test]
    fn center_union_mixes_bare_and_written_pairs() {
        let f = parsed("Alphabet a b c d ;\nRules\n\"r\" a | b:c | :d <= _ ;");
        assert_eq!(center(&f), [pair("a", "a"), pair("b", "c"), pair("?", "d")]);
    }

    #[test]
    fn bracketed_center_takes_bare_pairs_too() {
        let f = parsed("Alphabet a b c ;\nRules\n\"r\" [ a | b:c ] <= _ ;");
        assert_eq!(center(&f), [pair("a", "a"), pair("b", "c")]);
    }

    #[test]
    fn diagnostics_name_the_offending_token_not_its_debug_shape() {
        let e = parse("Alphabet a b ;\nRules\n\"r\" a b _ ;").expect_err("no arrow");
        let msg = &e.diagnostics[0].message;
        assert!(
            msg.contains("expected rule arrow (`=>`, `<=`, `<=>`, `/<=`), got `b`"),
            "{msg}"
        );
        assert!(!msg.contains("Some("), "{msg}");
    }

    #[test]
    fn diagnostics_name_end_of_input() {
        let e = parse("Alphabet a b ;\nRules\n\"r\" a:b").expect_err("truncated rule");
        let msg = &e.diagnostics[0].message;
        assert!(msg.contains("got end of input"), "{msg}");
        assert!(!msg.contains("None"), "{msg}");
    }

    #[test]
    fn except_clause_recorded() {
        let f = parsed("Alphabet a b c ;\nRules\n\"r\" a:b => c _ ;\nexcept b _ ;");
        assert_eq!(f.rules.len(), 1);
        assert_eq!(f.rules[0].value.positive_contexts.len(), 1);
        assert_eq!(f.rules[0].value.negative_contexts.len(), 1);
    }

    #[test]
    fn multi_context() {
        let f = parsed("Alphabet a b c d ;\nRules\n\"r\" a:b <=> _ c ;\n d _ ;\n");
        assert_eq!(f.rules[0].value.positive_contexts.len(), 2);
    }

    #[test]
    fn where_clause_with_matched() {
        let f = parsed(
            "Alphabet a b c d ;\nRules\n\"r\" V:Vy <=> _ ;\n\
             where V in (a c) and Vy in (b d) matched ;\n",
        );
        let vars = f.rules[0].value.variables.as_ref().unwrap();
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].matcher, VarMatcher::Freely); // implicit before `and`
        assert_eq!(vars[1].matcher, VarMatcher::Matched);
    }

    #[test]
    fn where_clause_bare_set_name() {
        // Upstream's `VAR_SYMBOL IN VAR_SYMBOL` production. The value is kept
        // verbatim; expanding `Vowels` to its members is the consumer's job.
        let f = parsed(
            "Alphabet a b ;\nSets\nVowels = a e i o u y ;\n\
             Rules\n\"r\" a:b <=> _ ;\nwhere V in Vowels matched ;\n",
        );
        let vars = f.rules[0].value.variables.as_ref().unwrap();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].matcher, VarMatcher::Matched);
        assert_eq!(vars[0].assignments.len(), 1);
        assert_eq!(vars[0].assignments[0].name, "V");
        assert_eq!(vars[0].assignments[0].values, ["Vowels"]);
    }

    #[test]
    fn where_clause_bare_name_and_parenthesised_forms_mix() {
        let f = parsed(
            "Alphabet a b c d ;\nSets\nVowels = a e ;\n\
             Rules\n\"r\" V:Vy <=> _ ;\nwhere V in Vowels and Vy in (b d) matched ;\n",
        );
        let vars = f.rules[0].value.variables.as_ref().unwrap();
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].assignments[0].values, ["Vowels"]);
        assert_eq!(vars[1].assignments[0].values, ["b", "d"]);
    }

    #[test]
    fn where_clause_bare_name_and_paren_list_agree() {
        let bare =
            parsed("Alphabet a b ;\nRules\n\"r\" a:b <=> _ ;\nwhere V in Vowels matched ;\n");
        let parens =
            parsed("Alphabet a b ;\nRules\n\"r\" a:b <=> _ ;\nwhere V in ( Vowels ) matched ;\n");
        assert_eq!(
            bare.rules[0].value.variables,
            parens.rules[0].value.variables
        );
    }

    #[test]
    fn where_clause_several_bare_assignments_in_one_block() {
        // Each bare form takes exactly one symbol, so a second `name in value`
        // simply starts the next assignment in the same block.
        let f = parsed(
            "Alphabet a b ;\nRules\n\"r\" a:b <=> _ ;\n\
             where V in Vowels C in Cons matched ;\n",
        );
        let vars = f.rules[0].value.variables.as_ref().unwrap();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].assignments.len(), 2);
        assert_eq!(vars[0].assignments[0].values, ["Vowels"]);
        assert_eq!(vars[0].assignments[1].values, ["Cons"]);
    }

    #[test]
    fn definitions_section() {
        let f = parsed("Alphabet a b ;\nDefinitions\nFoo = a b ;\nRules\n\"r\" a:b <=> _ ;\n");
        assert_eq!(f.definitions.len(), 1);
        assert_eq!(f.definitions[0].value.name, "Foo");
    }

    #[test]
    fn sets_section() {
        let f = parsed(
            "Alphabet a b c ;\nSets\nVowel = a e ;\nCons = b c ;\n\
             Rules\n\"r\" a:b <=> _ ;\n",
        );
        assert_eq!(f.sets.len(), 2);
    }

    #[test]
    fn diacritics_section() {
        let f = parsed(
            "Alphabet a b ;\nDiacritics @P.Foo.Bar@ ;\n\
             Rules\n\"r\" a:b <=> _ ;\n",
        );
        assert_eq!(f.diacritics.len(), 1);
    }
}
