//! Recursive-descent parser for pmatch. Mirrors the upstream
//! `pmatch_parse.yy`'s `EXPRESSION1` … `EXPRESSION13` ladder; each level
//! gets its own method, descending to the next on no-match.
//!
//! Top-level dispatch handles the five statement forms (`Define`,
//! `DefIns`, `regex`, `set`, `list`) plus the `@vec"…"` standalone
//! form.

use crate::ast::{
    Acceptor, BinaryOp, CaseOp, CaseSide, ContextMark, MappingKind, MappingPair, MappingSide,
    PmatchExpr, PmatchFile, PmatchReplaceRule, PmatchStatement, ReadKind, ReplaceArrow,
    ReplaceContext, ReplaceContexts, RestrContext, SpannedExpr, UnaryOp, VariableValue,
};
use crate::lexer::{LexError, tokenize};
use crate::token::Token;
use nfst_syntax::{Diagnostic, Span, Spanned};
use smol_str::SmolStr;

#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse(source: &str) -> Result<Spanned<PmatchFile>, ParseError> {
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

    // ───────────────────────── primitives ─────────────────────────

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
            _ => Err(self.err(format!("expected {label}, got {:?}", self.peek()))),
        }
    }

    fn spanned(value: PmatchExpr, span: Span) -> SpannedExpr {
        Spanned::new(value, span)
    }

    // ───────────────────────── top level ─────────────────────────

    fn parse_file(&mut self) -> Result<Spanned<PmatchFile>, Diagnostic> {
        let start = self.current_start();
        let mut statements = Vec::new();
        while !self.is_at_end() {
            let stmt = self.parse_statement()?;
            statements.push(stmt);
        }
        Ok(Spanned::new(PmatchFile { statements }, self.merge(start)))
    }

    fn parse_statement(&mut self) -> Result<Spanned<PmatchStatement>, Diagnostic> {
        let start = self.current_start();
        match self.peek() {
            Some(Token::Define) => self.parse_define(start),
            Some(Token::DefIns) => self.parse_defins(start),
            Some(Token::Regex) => self.parse_regex_top(start),
            Some(Token::SetVariable) => self.parse_set(start),
            Some(Token::DefinedList) => self.parse_list(start),
            Some(Token::ReadVec(_)) => {
                let (tok, _) = self.bump().unwrap();
                let path = match tok {
                    Token::ReadVec(s) => s,
                    _ => unreachable!(),
                };
                Ok(Spanned::new(
                    PmatchStatement::ReadVec { path },
                    self.merge(start),
                ))
            }
            other => Err(self.err(format!(
                "expected a statement (Define / DefIns / regex / set / list / @vec), got {other:?}"
            ))),
        }
    }

    fn parse_define(&mut self, start: usize) -> Result<Spanned<PmatchStatement>, Diagnostic> {
        self.bump(); // Define
        let name = match self.bump() {
            Some((Token::Symbol(s), _)) => s,
            Some((Token::SymbolWithLeftParen(s), _)) => {
                // Function definition: name + arglist + ) + body + ;
                let params = self.parse_arglist()?;
                self.expect(&Token::RightParenthesis, "`)`")?;
                let body = self.parse_expression1()?;
                return Ok(Spanned::new(
                    PmatchStatement::Define {
                        name: s,
                        params: Some(params),
                        body,
                    },
                    self.merge(start),
                ));
            }
            other => {
                return Err(Diagnostic::error(
                    self.peek_span(),
                    format!("expected definition name after `Define`, got {other:?}"),
                ));
            }
        };
        let body = self.parse_expression1()?;
        Ok(Spanned::new(
            PmatchStatement::Define {
                name,
                params: None,
                body,
            },
            self.merge(start),
        ))
    }

    fn parse_arglist(&mut self) -> Result<Vec<SmolStr>, Diagnostic> {
        let mut args = Vec::new();
        if matches!(self.peek(), Some(Token::RightParenthesis)) {
            return Ok(args);
        }
        loop {
            let arg = match self.bump() {
                Some((Token::Symbol(s), _)) => s,
                Some((Token::QuotedLiteral(s), _)) => s,
                other => {
                    return Err(Diagnostic::error(
                        self.peek_span(),
                        format!("expected argument name, got {other:?}"),
                    ));
                }
            };
            args.push(arg);
            if matches!(self.peek(), Some(Token::Comma)) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(args)
    }

    fn parse_defins(&mut self, start: usize) -> Result<Spanned<PmatchStatement>, Diagnostic> {
        self.bump(); // DefIns
        let name = match self.bump() {
            Some((Token::Symbol(s), _)) => s,
            other => {
                return Err(Diagnostic::error(
                    self.peek_span(),
                    format!("expected name after `DefIns`, got {other:?}"),
                ));
            }
        };
        let body = self.parse_expression1()?;
        Ok(Spanned::new(
            PmatchStatement::DefIns { name, body },
            self.merge(start),
        ))
    }

    fn parse_regex_top(&mut self, start: usize) -> Result<Spanned<PmatchStatement>, Diagnostic> {
        self.bump(); // regex
        let body = self.parse_expression1()?;
        Ok(Spanned::new(
            PmatchStatement::RegexTop { body },
            self.merge(start),
        ))
    }

    fn parse_set(&mut self, start: usize) -> Result<Spanned<PmatchStatement>, Diagnostic> {
        self.bump(); // set
        let name = match self.bump() {
            Some((Token::VariableName(s), _)) => s,
            Some((Token::Symbol(s), _)) => s,
            other => {
                return Err(Diagnostic::error(
                    self.peek_span(),
                    format!("expected variable name after `set`, got {other:?}"),
                ));
            }
        };
        let value = match self.bump() {
            Some((Token::Symbol(s), _)) => VariableValue::Symbol(s),
            Some((Token::EpsilonToken, _)) => VariableValue::Epsilon,
            other => {
                return Err(Diagnostic::error(
                    self.peek_span(),
                    format!("expected value after `set {name}`, got {other:?}"),
                ));
            }
        };
        Ok(Spanned::new(
            PmatchStatement::SetVariable { name, value },
            self.merge(start),
        ))
    }

    fn parse_list(&mut self, start: usize) -> Result<Spanned<PmatchStatement>, Diagnostic> {
        self.bump(); // list
        let name = match self.bump() {
            Some((Token::Symbol(s), _)) => s,
            other => {
                return Err(Diagnostic::error(
                    self.peek_span(),
                    format!("expected list name after `list`, got {other:?}"),
                ));
            }
        };
        let body = self.parse_expression1()?;
        Ok(Spanned::new(
            PmatchStatement::ListDefinition { name, body },
            self.merge(start),
        ))
    }

    // ───────────────────────── expression1 ─────────────────────────
    // EXPRESSION1: EXPRESSION2 END_OF_WEIGHTED_EXPRESSION(w)
    fn parse_expression1(&mut self) -> Result<SpannedExpr, Diagnostic> {
        let start = self.current_start();
        let mut expr = self.parse_expression2()?;
        match self.bump() {
            Some((Token::EndOfWeightedExpression(w), _)) => {
                if w != 0.0 {
                    expr = Self::spanned(
                        PmatchExpr::Weighted {
                            expr: Box::new(expr),
                            weight: w,
                        },
                        self.merge(start),
                    );
                }
                Ok(expr)
            }
            other => Err(Diagnostic::error(
                self.peek_span(),
                format!("expected `;` after expression, got {other:?}"),
            )),
        }
    }

    // ───────────────────────── expression2 ─────────────────────────
    // EXPRESSION2: EXPRESSION3 (.o./.x./.O./.m>./.<m. EXPRESSION3)*
    //            | substitute   (` [ expr3 , expr3 , expr3 ])
    //            | expr2 PAIR_SEPARATOR_WO_RIGHT       (expr:?)
    //            | PAIR_SEPARATOR_WO_LEFT expr2        (?:expr)
    //            | PAIR_SEPARATOR_SOLE                 (?:?)
    fn parse_expression2(&mut self) -> Result<SpannedExpr, Diagnostic> {
        let start = self.current_start();

        if matches!(self.peek(), Some(Token::SubstituteLeft)) {
            return self.parse_substitute(start);
        }

        // ?:?  /  ?:expr  forms with leading separator
        if matches!(self.peek(), Some(Token::PairSeparatorSole)) {
            self.bump();
            return Ok(Self::spanned(
                PmatchExpr::Pair {
                    upper: Box::new(Self::spanned(PmatchExpr::Any, self.peek_span())),
                    lower: Box::new(Self::spanned(PmatchExpr::Any, self.peek_span())),
                },
                self.merge(start),
            ));
        }
        if matches!(self.peek(), Some(Token::PairSeparatorWoLeft)) {
            self.bump();
            let rhs = self.parse_expression2()?;
            return Ok(Self::spanned(
                PmatchExpr::Pair {
                    upper: Box::new(Self::spanned(PmatchExpr::Any, self.peek_span())),
                    lower: Box::new(rhs),
                },
                self.merge(start),
            ));
        }

        let mut left = self.parse_expression3()?;
        loop {
            let op = match self.peek() {
                Some(Token::Composition) => Some(BinaryOp::Compose),
                Some(Token::CrossProduct) => Some(BinaryOp::CrossProduct),
                Some(Token::LenientComposition) => Some(BinaryOp::LenientCompose),
                Some(Token::MergeRightArrow) => Some(BinaryOp::MergeRight),
                Some(Token::MergeLeftArrow) => Some(BinaryOp::MergeLeft),
                _ => None,
            };
            if let Some(op) = op {
                self.bump();
                let right = self.parse_expression3()?;
                left = Self::spanned(
                    PmatchExpr::Binary(op, Box::new(left), Box::new(right)),
                    self.merge(start),
                );
                continue;
            }
            // Postfix `:?`: expression followed by PAIR_SEPARATOR_WO_RIGHT.
            if matches!(self.peek(), Some(Token::PairSeparatorWoRight)) {
                self.bump();
                left = Self::spanned(
                    PmatchExpr::Pair {
                        upper: Box::new(left),
                        lower: Box::new(Self::spanned(PmatchExpr::Any, self.peek_span())),
                    },
                    self.merge(start),
                );
                continue;
            }
            break;
        }
        Ok(left)
    }

    fn parse_substitute(&mut self, start: usize) -> Result<SpannedExpr, Diagnostic> {
        self.expect(&Token::SubstituteLeft, "`")?;
        self.expect(&Token::LeftBracket, "`[`")?;
        let a = self.parse_expression3()?;
        self.expect(&Token::Comma, "`,`")?;
        let b = self.parse_expression3()?;
        self.expect(&Token::Comma, "`,`")?;
        let c = self.parse_expression3()?;
        self.expect(&Token::RightBracket, "`]`")?;
        Ok(Self::spanned(
            PmatchExpr::Substitute(Box::new(a), Box::new(b), Box::new(c)),
            self.merge(start),
        ))
    }

    // ───────────────────────── expression3 ─────────────────────────
    // EXPRESSION3: EXPRESSION4 | PARALLEL_RULES
    fn parse_expression3(&mut self) -> Result<SpannedExpr, Diagnostic> {
        let start = self.current_start();
        let first = self.parse_expression4()?;

        if let Some(arrow) = self.peek_replace_arrow() {
            self.bump();
            let mapping = self.parse_mapping_after_arrow(MappingSide::from_expr(first))?;
            let mut mappings = vec![mapping];
            while matches!(self.peek(), Some(Token::Comma)) && self.peek_starts_mapping_lhs_at(1) {
                self.bump();
                let upper = MappingSide::from_expr(self.parse_expression4()?);
                let arrow2 = self.expect_replace_arrow()?;
                if arrow2 != arrow {
                    return Err(self.err("replace arrows in a parallel rule list must match"));
                }
                mappings.push(self.parse_mapping_after_arrow(upper)?);
            }
            let contexts = self.try_parse_replace_contexts()?;
            let mut rules = vec![PmatchReplaceRule { mappings, contexts }];
            while matches!(self.peek(), Some(Token::Commacomma)) {
                self.bump();
                rules.push(self.parse_replace_rule(arrow)?);
            }
            return Ok(Self::spanned(
                PmatchExpr::Replace { arrow, rules },
                self.merge(start),
            ));
        }
        Ok(first)
    }

    fn parse_replace_rule(
        &mut self,
        expected_arrow: ReplaceArrow,
    ) -> Result<PmatchReplaceRule, Diagnostic> {
        let upper = MappingSide::from_expr(self.parse_expression4()?);
        let arrow = self.expect_replace_arrow()?;
        if arrow != expected_arrow {
            return Err(self.err("replace arrows in parallel rules must match"));
        }
        let mut mappings = vec![self.parse_mapping_after_arrow(upper)?];
        while matches!(self.peek(), Some(Token::Comma)) && self.peek_starts_mapping_lhs_at(1) {
            self.bump();
            let u = MappingSide::from_expr(self.parse_expression4()?);
            let a2 = self.expect_replace_arrow()?;
            if a2 != expected_arrow {
                return Err(self.err("replace arrows in parallel rules must match"));
            }
            mappings.push(self.parse_mapping_after_arrow(u)?);
        }
        let contexts = self.try_parse_replace_contexts()?;
        Ok(PmatchReplaceRule { mappings, contexts })
    }

    fn parse_mapping_after_arrow(&mut self, upper: MappingSide) -> Result<MappingPair, Diagnostic> {
        if matches!(self.peek(), Some(Token::MarkupMarker)) {
            self.bump();
            let post = MappingSide::from_expr(self.parse_expression4()?);
            return Ok(MappingPair {
                upper,
                kind: MappingKind::Markup {
                    pre: None,
                    post: Some(post),
                },
            });
        }
        let lower = MappingSide::from_expr(self.parse_expression4()?);
        if matches!(self.peek(), Some(Token::MarkupMarker)) {
            self.bump();
            let post = if self.peek_starts_atom() {
                Some(MappingSide::from_expr(self.parse_expression4()?))
            } else {
                None
            };
            return Ok(MappingPair {
                upper,
                kind: MappingKind::Markup {
                    pre: Some(lower),
                    post,
                },
            });
        }
        Ok(MappingPair {
            upper,
            kind: MappingKind::Plain { lower },
        })
    }

    fn peek_replace_arrow(&self) -> Option<ReplaceArrow> {
        match self.peek() {
            Some(Token::ReplaceRight) => Some(ReplaceArrow::Right),
            Some(Token::OptionalReplaceRight) => Some(ReplaceArrow::OptionalRight),
            Some(Token::ReplaceLeft) => Some(ReplaceArrow::Left),
            Some(Token::OptionalReplaceLeft) => Some(ReplaceArrow::OptionalLeft),
            Some(Token::ReplaceLeftRight) => Some(ReplaceArrow::LeftRight),
            Some(Token::OptionalReplaceLeftRight) => Some(ReplaceArrow::OptionalLeftRight),
            Some(Token::LtrLongestMatch) => Some(ReplaceArrow::LtrLongest),
            Some(Token::LtrShortestMatch) => Some(ReplaceArrow::LtrShortest),
            Some(Token::RtlLongestMatch) => Some(ReplaceArrow::RtlLongest),
            Some(Token::RtlShortestMatch) => Some(ReplaceArrow::RtlShortest),
            _ => None,
        }
    }

    fn expect_replace_arrow(&mut self) -> Result<ReplaceArrow, Diagnostic> {
        match self.peek_replace_arrow() {
            Some(a) => {
                self.bump();
                Ok(a)
            }
            None => Err(self.err("expected a replace arrow")),
        }
    }

    fn try_parse_replace_contexts(&mut self) -> Result<Option<ReplaceContexts>, Diagnostic> {
        let mark = match self.peek() {
            Some(Token::ReplaceContextUu) => ContextMark::UpperUpper,
            Some(Token::ReplaceContextLu) => ContextMark::LowerUpper,
            Some(Token::ReplaceContextUl) => ContextMark::UpperLower,
            Some(Token::ReplaceContextLl) => ContextMark::LowerLower,
            _ => return Ok(None),
        };
        self.bump();
        let mut items = Vec::new();
        loop {
            items.push(self.parse_replace_context()?);
            if matches!(self.peek(), Some(Token::Comma)) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(Some(ReplaceContexts { mark, items }))
    }

    fn parse_replace_context(&mut self) -> Result<ReplaceContext, Diagnostic> {
        if matches!(self.peek(), Some(Token::CenterMarker)) {
            self.bump();
            if self.peek_starts_atom() {
                let r = self.parse_expression3()?;
                return Ok(ReplaceContext {
                    left: None,
                    right: Some(Box::new(r)),
                });
            }
            return Ok(ReplaceContext {
                left: None,
                right: None,
            });
        }
        let left = self.parse_expression3()?;
        self.expect(&Token::CenterMarker, "`_`")?;
        if self.peek_starts_atom() {
            let r = self.parse_expression3()?;
            Ok(ReplaceContext {
                left: Some(Box::new(left)),
                right: Some(Box::new(r)),
            })
        } else {
            Ok(ReplaceContext {
                left: Some(Box::new(left)),
                right: None,
            })
        }
    }

    fn peek_starts_mapping_lhs_at(&self, off: usize) -> bool {
        matches!(
            self.tokens.get(self.pos + off).map(|(t, _)| t),
            Some(
                Token::Symbol(_)
                    | Token::SymbolWithLeftParen(_)
                    | Token::QuotedLiteral(_)
                    | Token::CurlyLiteral(_)
                    | Token::CharacterRange(_, _)
                    | Token::EpsilonToken
                    | Token::AnyToken
                    | Token::BoundaryMarker
                    | Token::LeftBracket
                    | Token::LeftParenthesis
                    | Token::LeftBracketDotted
                    | Token::Complement
                    | Token::TermComplement
                    | Token::Containment
                    | Token::ContainmentOnce
                    | Token::ContainmentOpt
                    | Token::Alpha
                    | Token::UppercaseAlpha
                    | Token::LowercaseAlpha
                    | Token::Num
                    | Token::Punct
                    | Token::Whitespace
                    | Token::LitLeft
                    | Token::InsLeft
                    | Token::EndTagLeft
                    | Token::CaptureLeft
                    | Token::CapLeft
                    | Token::OptCapLeft
                    | Token::ToLowerLeft
                    | Token::ToUpperLeft
                    | Token::OptToLowerLeft
                    | Token::OptToUpperLeft
                    | Token::AnyCaseLeft
                    | Token::ExplodeLeft
                    | Token::ImplodeLeft
                    | Token::LcLeft
                    | Token::RcLeft
                    | Token::NlcLeft
                    | Token::NrcLeft
                    | Token::OrLeft
                    | Token::AndLeft
                    | Token::LstLeft
                    | Token::ExcLeft
                    | Token::LikeLeft
                    | Token::UnlikeLeft
                    | Token::InterpolateLeft
                    | Token::SigmaLeft
                    | Token::CounterLeft
                    | Token::DefineLeft
                    | Token::UncomposeLeft
                    | Token::TagLeft
                    | Token::WithLeft
                    | Token::ReadBin(_)
                    | Token::ReadText(_)
                    | Token::ReadSpaced(_)
                    | Token::ReadProlog(_)
                    | Token::ReadLexc(_)
                    | Token::ReadRe(_)
                    | Token::ReadVec(_)
            )
        )
    }

    fn peek_starts_atom(&self) -> bool {
        self.peek_starts_mapping_lhs_at(0)
    }

    // ───────────────────────── expression4 ─────────────────────────
    fn parse_expression4(&mut self) -> Result<SpannedExpr, Diagnostic> {
        let start = self.current_start();
        let mut left = self.parse_expression5()?;
        loop {
            let op = match self.peek() {
                Some(Token::Shuffle) => BinaryOp::Shuffle,
                Some(Token::Before) => BinaryOp::Before,
                Some(Token::After) => BinaryOp::After,
                _ => break,
            };
            self.bump();
            let right = self.parse_expression5()?;
            left = Self::spanned(
                PmatchExpr::Binary(op, Box::new(left), Box::new(right)),
                self.merge(start),
            );
        }
        Ok(left)
    }

    // ───────────────────────── expression5 ─────────────────────────
    fn parse_expression5(&mut self) -> Result<SpannedExpr, Diagnostic> {
        let start = self.current_start();
        let body = self.parse_expression6()?;
        if matches!(self.peek(), Some(Token::RightArrow)) {
            self.bump();
            let mut contexts = Vec::new();
            loop {
                contexts.push(self.parse_restr_context()?);
                if matches!(self.peek(), Some(Token::Comma)) {
                    self.bump();
                } else {
                    break;
                }
            }
            return Ok(Self::spanned(
                PmatchExpr::Restriction {
                    body: Box::new(body),
                    contexts,
                },
                self.merge(start),
            ));
        }
        Ok(body)
    }

    fn parse_restr_context(&mut self) -> Result<RestrContext, Diagnostic> {
        if matches!(self.peek(), Some(Token::CenterMarker)) {
            self.bump();
            if self.peek_starts_atom() {
                let r = self.parse_expression6()?;
                return Ok(RestrContext {
                    left: None,
                    right: Some(Box::new(r)),
                });
            }
            return Ok(RestrContext {
                left: None,
                right: None,
            });
        }
        let left = self.parse_expression6()?;
        self.expect(&Token::CenterMarker, "`_`")?;
        if self.peek_starts_atom() {
            let r = self.parse_expression6()?;
            Ok(RestrContext {
                left: Some(Box::new(left)),
                right: Some(Box::new(r)),
            })
        } else {
            Ok(RestrContext {
                left: Some(Box::new(left)),
                right: None,
            })
        }
    }

    // ───────────────────────── expression6 ─────────────────────────
    fn parse_expression6(&mut self) -> Result<SpannedExpr, Diagnostic> {
        let start = self.current_start();
        let mut left = self.parse_expression7()?;
        loop {
            let op = match self.peek() {
                Some(Token::Union) => BinaryOp::Union,
                Some(Token::Intersection) => BinaryOp::Intersect,
                Some(Token::Minus) => BinaryOp::Subtract,
                Some(Token::UpperMinus) => BinaryOp::UpperSubtract,
                Some(Token::LowerMinus) => BinaryOp::LowerSubtract,
                Some(Token::UpperPriorityUnion) => BinaryOp::UpperPriorityUnion,
                Some(Token::LowerPriorityUnion) => BinaryOp::LowerPriorityUnion,
                _ => break,
            };
            self.bump();
            let right = self.parse_expression7()?;
            left = Self::spanned(
                PmatchExpr::Binary(op, Box::new(left), Box::new(right)),
                self.merge(start),
            );
        }
        Ok(left)
    }

    // ───────────────────────── expression7 ─────────────────────────
    // Concatenation by juxtaposition. The grammar rule is ambiguous
    // (`EXPRESSION7: EXPRESSION7 EXPRESSION7`) and bison resolves the
    // shift/reduce conflict by shifting, so a chain nests to the right:
    // `A B C` is Concat(A, Concat(B, C)). Analyses that inspect the left
    // child of a concatenation — pmatch's context detection among them —
    // depend on that shape.
    fn parse_expression7(&mut self) -> Result<SpannedExpr, Diagnostic> {
        let start = self.current_start();
        let first = self.parse_expression8()?;
        if !self.peek_starts_atom() {
            return Ok(first);
        }
        let mut operands = vec![(start, first)];
        while self.peek_starts_atom() {
            let operand_start = self.current_start();
            operands.push((operand_start, self.parse_expression8()?));
        }
        // Folded right after the fact rather than by recursing, so that a long
        // juxtaposition chain costs no parser stack.
        let (_, mut acc) = operands.pop().expect("chain has at least two operands");
        while let Some((operand_start, left)) = operands.pop() {
            acc = Self::spanned(
                PmatchExpr::Binary(BinaryOp::Concatenate, Box::new(left), Box::new(acc)),
                self.merge(operand_start),
            );
        }
        Ok(acc)
    }

    // ───────────────────────── expression8 ─────────────────────────
    fn parse_expression8(&mut self) -> Result<SpannedExpr, Diagnostic> {
        let start = self.current_start();
        let mut left = self.parse_expression9()?;
        loop {
            let op = match self.peek() {
                Some(Token::Ignoring) => BinaryOp::Ignoring,
                Some(Token::IgnoreInternally) => BinaryOp::IgnoreInternally,
                Some(Token::LeftQuotient) => BinaryOp::LeftQuotient,
                _ => break,
            };
            self.bump();
            let right = self.parse_expression9()?;
            left = Self::spanned(
                PmatchExpr::Binary(op, Box::new(left), Box::new(right)),
                self.merge(start),
            );
        }
        Ok(left)
    }

    // ───────────────────────── expression9 ─────────────────────────
    // Prefix unaries: ~, $, $., $?
    fn parse_expression9(&mut self) -> Result<SpannedExpr, Diagnostic> {
        let start = self.current_start();
        match self.peek() {
            Some(Token::Complement) => {
                self.bump();
                let inner = self.parse_expression10()?;
                Ok(Self::spanned(
                    PmatchExpr::Unary(UnaryOp::Complement, Box::new(inner)),
                    self.merge(start),
                ))
            }
            Some(Token::Containment) => {
                self.bump();
                let inner = self.parse_expression10()?;
                Ok(Self::spanned(
                    PmatchExpr::Unary(UnaryOp::Containment, Box::new(inner)),
                    self.merge(start),
                ))
            }
            Some(Token::ContainmentOnce) => {
                self.bump();
                let inner = self.parse_expression10()?;
                Ok(Self::spanned(
                    PmatchExpr::Unary(UnaryOp::ContainmentOnce, Box::new(inner)),
                    self.merge(start),
                ))
            }
            Some(Token::ContainmentOpt) => {
                self.bump();
                let inner = self.parse_expression10()?;
                Ok(Self::spanned(
                    PmatchExpr::Unary(UnaryOp::ContainmentOpt, Box::new(inner)),
                    self.merge(start),
                ))
            }
            _ => self.parse_expression10(),
        }
    }

    // ───────────────────────── expression10 ─────────────────────────
    // Postfix: *, +, .r, .i, .u, .l, ^N, ^>N, ^<N, ^N,K
    fn parse_expression10(&mut self) -> Result<SpannedExpr, Diagnostic> {
        let start = self.current_start();
        let mut expr = self.parse_expression11()?;
        loop {
            let next = self.peek().cloned();
            expr = match next {
                Some(Token::Star) => {
                    self.bump();
                    Self::spanned(
                        PmatchExpr::Unary(UnaryOp::Star, Box::new(expr)),
                        self.merge(start),
                    )
                }
                Some(Token::Plus) => {
                    self.bump();
                    Self::spanned(
                        PmatchExpr::Unary(UnaryOp::Plus, Box::new(expr)),
                        self.merge(start),
                    )
                }
                Some(Token::Reverse) => {
                    self.bump();
                    Self::spanned(
                        PmatchExpr::Unary(UnaryOp::Reverse, Box::new(expr)),
                        self.merge(start),
                    )
                }
                Some(Token::Invert) => {
                    self.bump();
                    Self::spanned(
                        PmatchExpr::Unary(UnaryOp::Invert, Box::new(expr)),
                        self.merge(start),
                    )
                }
                Some(Token::UpperProject) => {
                    self.bump();
                    Self::spanned(
                        PmatchExpr::Unary(UnaryOp::UpperProject, Box::new(expr)),
                        self.merge(start),
                    )
                }
                Some(Token::LowerProject) => {
                    self.bump();
                    Self::spanned(
                        PmatchExpr::Unary(UnaryOp::LowerProject, Box::new(expr)),
                        self.merge(start),
                    )
                }
                Some(Token::CatenateN(_))
                | Some(Token::CatenateNPlus(_))
                | Some(Token::CatenateNMinus(_))
                | Some(Token::CatenateNToK(_, _)) => {
                    let (tok, _) = self.bump().unwrap();
                    let span = self.merge(start);
                    let v = match tok {
                        Token::CatenateN(n) => PmatchExpr::RepeatN(Box::new(expr), n),
                        Token::CatenateNPlus(n) => PmatchExpr::RepeatNPlus(Box::new(expr), n),
                        Token::CatenateNMinus(n) => PmatchExpr::RepeatNMinus(Box::new(expr), n),
                        Token::CatenateNToK(n, k) => PmatchExpr::RepeatNToK(Box::new(expr), n, k),
                        _ => unreachable!(),
                    };
                    Self::spanned(v, span)
                }
                _ => break,
            };
        }
        Ok(expr)
    }

    // ───────────────────────── expression11 ─────────────────────────
    // Term-complement prefix: \E
    fn parse_expression11(&mut self) -> Result<SpannedExpr, Diagnostic> {
        let start = self.current_start();
        if matches!(self.peek(), Some(Token::TermComplement)) {
            self.bump();
            let inner = self.parse_expression12()?;
            return Ok(Self::spanned(
                PmatchExpr::Unary(UnaryOp::TermComplement, Box::new(inner)),
                self.merge(start),
            ));
        }
        self.parse_expression12()
    }

    // ───────────────────────── expression12 ─────────────────────────
    // [ E ] (with optional :pair, weight, .t(name), .with(k=v))
    // ( E ) — optionalized
    fn parse_expression12(&mut self) -> Result<SpannedExpr, Diagnostic> {
        let start = self.current_start();
        match self.peek() {
            Some(Token::LeftBracket) => {
                self.bump();
                let inner = self.parse_expression2()?;
                self.expect(&Token::RightBracket, "`]`")?;
                let mut e = Self::spanned(PmatchExpr::Group(Box::new(inner)), self.merge(start));
                // Postfix forms specific to bracketed groups.
                e = self.parse_bracketed_postfix(e, start)?;
                self.parse_expression12_tail(e, start)
            }
            Some(Token::LeftParenthesis) => {
                self.bump();
                let inner = self.parse_expression2()?;
                self.expect(&Token::RightParenthesis, "`)`")?;
                let e = Self::spanned(PmatchExpr::Optional(Box::new(inner)), self.merge(start));
                self.parse_expression12_tail(e, start)
            }
            Some(Token::LeftBracketDotted) => {
                self.bump();
                if matches!(self.peek(), Some(Token::RightBracketDotted)) {
                    self.bump();
                    Ok(Self::spanned(
                        PmatchExpr::BracketedDotted(None),
                        self.merge(start),
                    ))
                } else {
                    let inner = self.parse_expression2()?;
                    self.expect(&Token::RightBracketDotted, "`.]`")?;
                    Ok(Self::spanned(
                        PmatchExpr::BracketedDotted(Some(Box::new(inner))),
                        self.merge(start),
                    ))
                }
            }
            _ => {
                let e = self.parse_expression13()?;
                self.parse_expression12_tail(e, start)
            }
        }
    }

    /// Postfix for any expression12: `: e12`, `WEIGHT`.
    fn parse_expression12_tail(
        &mut self,
        mut e: SpannedExpr,
        start: usize,
    ) -> Result<SpannedExpr, Diagnostic> {
        loop {
            match self.peek() {
                Some(Token::PairSeparator) => {
                    self.bump();
                    let rhs = self.parse_expression12()?;
                    e = Self::spanned(
                        PmatchExpr::Pair {
                            upper: Box::new(e),
                            lower: Box::new(rhs),
                        },
                        self.merge(start),
                    );
                }
                Some(Token::Weight(_)) => {
                    let (tok, _) = self.bump().unwrap();
                    let w = match tok {
                        Token::Weight(w) => w,
                        _ => unreachable!(),
                    };
                    e = Self::spanned(
                        PmatchExpr::Weighted {
                            expr: Box::new(e),
                            weight: w,
                        },
                        self.merge(start),
                    );
                }
                _ => break,
            }
        }
        Ok(e)
    }

    /// Postfix for `[E]` specifically: `.t(name)`, `.with(name = value)`.
    fn parse_bracketed_postfix(
        &mut self,
        e: SpannedExpr,
        start: usize,
    ) -> Result<SpannedExpr, Diagnostic> {
        if matches!(self.peek(), Some(Token::TagLeft)) {
            self.bump();
            let name = match self.bump() {
                Some((Token::Symbol(s), _)) | Some((Token::QuotedLiteral(s), _)) => s,
                other => {
                    return Err(Diagnostic::error(
                        self.peek_span(),
                        format!("expected tag name, got {other:?}"),
                    ));
                }
            };
            self.expect(&Token::RightParenthesis, "`)`")?;
            return Ok(Self::spanned(
                PmatchExpr::Tag {
                    body: Box::new(e),
                    name,
                },
                self.merge(start),
            ));
        }
        if matches!(self.peek(), Some(Token::WithLeft)) {
            self.bump();
            let name = match self.bump() {
                Some((Token::Symbol(s), _)) => s,
                other => {
                    return Err(Diagnostic::error(
                        self.peek_span(),
                        format!("expected variable name in `.with(`, got {other:?}"),
                    ));
                }
            };
            self.expect(&Token::Equals, "`=`")?;
            let value = match self.bump() {
                Some((Token::Symbol(s), _)) => s,
                other => {
                    return Err(Diagnostic::error(
                        self.peek_span(),
                        format!("expected value in `.with(`, got {other:?}"),
                    ));
                }
            };
            self.expect(&Token::RightParenthesis, "`)`")?;
            return Ok(Self::spanned(
                PmatchExpr::With {
                    body: Box::new(e),
                    name,
                    value,
                },
                self.merge(start),
            ));
        }
        Ok(e)
    }

    // ───────────────────────── expression13 (atoms) ─────────────────────────
    fn parse_expression13(&mut self) -> Result<SpannedExpr, Diagnostic> {
        let start = self.current_start();
        let next = self.peek().cloned();
        let result = match next {
            Some(Token::Symbol(_)) => {
                let (tok, _) = self.bump().unwrap();
                let s = match tok {
                    Token::Symbol(s) => s,
                    _ => unreachable!(),
                };
                PmatchExpr::Symbol(s)
            }
            Some(Token::QuotedLiteral(_)) => {
                let (tok, _) = self.bump().unwrap();
                let s = match tok {
                    Token::QuotedLiteral(s) => s,
                    _ => unreachable!(),
                };
                PmatchExpr::QuotedLiteral(s)
            }
            Some(Token::CurlyLiteral(_)) => {
                let (tok, _) = self.bump().unwrap();
                let s = match tok {
                    Token::CurlyLiteral(s) => s,
                    _ => unreachable!(),
                };
                PmatchExpr::CurlyLiteral(s)
            }
            Some(Token::CharacterRange(_, _)) => {
                let (tok, _) = self.bump().unwrap();
                let (a, b) = match tok {
                    Token::CharacterRange(a, b) => (a, b),
                    _ => unreachable!(),
                };
                PmatchExpr::CharacterRange { from: a, to: b }
            }
            Some(Token::EpsilonToken) => {
                self.bump();
                PmatchExpr::Epsilon
            }
            Some(Token::AnyToken) => {
                self.bump();
                PmatchExpr::Any
            }
            Some(Token::BoundaryMarker) => {
                self.bump();
                PmatchExpr::BoundaryMarker
            }
            Some(Token::Alpha) => {
                self.bump();
                PmatchExpr::Acceptor(Acceptor::Alpha)
            }
            Some(Token::UppercaseAlpha) => {
                self.bump();
                PmatchExpr::Acceptor(Acceptor::UppercaseAlpha)
            }
            Some(Token::LowercaseAlpha) => {
                self.bump();
                PmatchExpr::Acceptor(Acceptor::LowercaseAlpha)
            }
            Some(Token::Num) => {
                self.bump();
                PmatchExpr::Acceptor(Acceptor::Num)
            }
            Some(Token::Punct) => {
                self.bump();
                PmatchExpr::Acceptor(Acceptor::Punct)
            }
            Some(Token::Whitespace) => {
                self.bump();
                PmatchExpr::Acceptor(Acceptor::Whitespace)
            }
            // ─── function-call keyword forms ───
            Some(Token::LitLeft) => return self.parse_lit(start),
            Some(Token::InsLeft) => return self.parse_ins(start),
            Some(Token::EndTagLeft) => return self.parse_end_tag(start),
            Some(Token::CaptureLeft) => return self.parse_capture(start),
            Some(Token::CounterLeft) => return self.parse_counter(start),
            Some(Token::CapLeft) => return self.parse_case_op(CaseOp::Cap, start),
            Some(Token::OptCapLeft) => return self.parse_case_op(CaseOp::OptCap, start),
            Some(Token::ToLowerLeft) => return self.parse_case_op(CaseOp::ToLower, start),
            Some(Token::ToUpperLeft) => return self.parse_case_op(CaseOp::ToUpper, start),
            Some(Token::OptToLowerLeft) => return self.parse_case_op(CaseOp::OptToLower, start),
            Some(Token::OptToUpperLeft) => return self.parse_case_op(CaseOp::OptToUpper, start),
            Some(Token::AnyCaseLeft) => return self.parse_case_op(CaseOp::AnyCase, start),
            Some(Token::ExplodeLeft) => return self.parse_explode_implode(true, start),
            Some(Token::ImplodeLeft) => return self.parse_explode_implode(false, start),
            Some(Token::LcLeft) => return self.parse_unary_call(PmatchExpr::Lc, start),
            Some(Token::RcLeft) => return self.parse_unary_call(PmatchExpr::Rc, start),
            Some(Token::NlcLeft) => return self.parse_unary_call(PmatchExpr::Nlc, start),
            Some(Token::NrcLeft) => return self.parse_unary_call(PmatchExpr::Nrc, start),
            Some(Token::OrLeft) => return self.parse_or_and(true, start),
            Some(Token::AndLeft) => return self.parse_or_and(false, start),
            Some(Token::LstLeft) => return self.parse_unary_call(PmatchExpr::Lst, start),
            Some(Token::ExcLeft) => return self.parse_unary_call(PmatchExpr::Exc, start),
            Some(Token::SigmaLeft) => return self.parse_unary_call(PmatchExpr::Sigma, start),
            Some(Token::DefineLeft) => {
                return self.parse_unary_call(PmatchExpr::DefineWrapper, start);
            }
            Some(Token::LikeLeft) => return self.parse_like(false, start),
            Some(Token::UnlikeLeft) => return self.parse_like(true, start),
            Some(Token::InterpolateLeft) => return self.parse_interpolate(start),
            Some(Token::UncomposeLeft) => return self.parse_uncompose(start),
            Some(Token::SymbolWithLeftParen(_)) => return self.parse_user_call(start),
            // file references
            Some(Token::ReadBin(_)) => {
                let (tok, _) = self.bump().unwrap();
                let p = match tok {
                    Token::ReadBin(s) => s,
                    _ => unreachable!(),
                };
                PmatchExpr::ReadFile {
                    kind: ReadKind::Binary,
                    path: p,
                }
            }
            Some(Token::ReadText(_)) => {
                let (tok, _) = self.bump().unwrap();
                let p = match tok {
                    Token::ReadText(s) => s,
                    _ => unreachable!(),
                };
                PmatchExpr::ReadFile {
                    kind: ReadKind::Text,
                    path: p,
                }
            }
            Some(Token::ReadSpaced(_)) => {
                let (tok, _) = self.bump().unwrap();
                let p = match tok {
                    Token::ReadSpaced(s) => s,
                    _ => unreachable!(),
                };
                PmatchExpr::ReadFile {
                    kind: ReadKind::Spaced,
                    path: p,
                }
            }
            Some(Token::ReadProlog(_)) => {
                let (tok, _) = self.bump().unwrap();
                let p = match tok {
                    Token::ReadProlog(s) => s,
                    _ => unreachable!(),
                };
                PmatchExpr::ReadFile {
                    kind: ReadKind::Prolog,
                    path: p,
                }
            }
            Some(Token::ReadRe(_)) => {
                let (tok, _) = self.bump().unwrap();
                let p = match tok {
                    Token::ReadRe(s) => s,
                    _ => unreachable!(),
                };
                PmatchExpr::ReadFile {
                    kind: ReadKind::Regex,
                    path: p,
                }
            }
            Some(Token::ReadLexc(_)) => {
                let (tok, _) = self.bump().unwrap();
                let p = match tok {
                    Token::ReadLexc(s) => s,
                    _ => unreachable!(),
                };
                PmatchExpr::ReadLexc(p)
            }
            Some(Token::ReadVec(_)) => {
                let (tok, _) = self.bump().unwrap();
                let p = match tok {
                    Token::ReadVec(s) => s,
                    _ => unreachable!(),
                };
                PmatchExpr::ReadVec(p)
            }
            other => {
                return Err(Diagnostic::error(
                    self.peek_span(),
                    format!("unexpected token in expression: {other:?}"),
                ));
            }
        };
        Ok(Self::spanned(result, self.merge(start)))
    }

    // ───────────────────────── helpers for each call form ─────────────────────────

    fn parse_lit(&mut self, start: usize) -> Result<SpannedExpr, Diagnostic> {
        self.bump(); // LitLeft
        let s = self.expect_name_atom()?;
        self.expect(&Token::RightParenthesis, "`)`")?;
        Ok(Self::spanned(PmatchExpr::Literal(s), self.merge(start)))
    }

    fn parse_ins(&mut self, start: usize) -> Result<SpannedExpr, Diagnostic> {
        self.bump();
        let s = self.expect_name_atom()?;
        self.expect(&Token::RightParenthesis, "`)`")?;
        Ok(Self::spanned(PmatchExpr::Ins(s), self.merge(start)))
    }

    fn parse_end_tag(&mut self, start: usize) -> Result<SpannedExpr, Diagnostic> {
        self.bump();
        let s = self.expect_name_atom()?;
        self.expect(&Token::RightParenthesis, "`)`")?;
        Ok(Self::spanned(PmatchExpr::EndTag(s), self.merge(start)))
    }

    fn parse_capture(&mut self, start: usize) -> Result<SpannedExpr, Diagnostic> {
        self.bump();
        let s = self.expect_name_atom()?;
        self.expect(&Token::RightParenthesis, "`)`")?;
        Ok(Self::spanned(PmatchExpr::Capture(s), self.merge(start)))
    }

    fn parse_counter(&mut self, start: usize) -> Result<SpannedExpr, Diagnostic> {
        self.bump();
        let s = self.expect_name_atom()?;
        self.expect(&Token::RightParenthesis, "`)`")?;
        Ok(Self::spanned(PmatchExpr::Counter(s), self.merge(start)))
    }

    fn expect_name_atom(&mut self) -> Result<SmolStr, Diagnostic> {
        match self.bump() {
            Some((Token::Symbol(s), _))
            | Some((Token::QuotedLiteral(s), _))
            | Some((Token::CurlyLiteral(s), _)) => Ok(s),
            other => Err(Diagnostic::error(
                self.peek_span(),
                format!("expected a name (Symbol/QuotedLiteral/CurlyLiteral), got {other:?}"),
            )),
        }
    }

    fn parse_case_op(&mut self, op: CaseOp, start: usize) -> Result<SpannedExpr, Diagnostic> {
        self.bump(); // <op>Left
        let body = self.parse_expression2()?;
        let side = if matches!(self.peek(), Some(Token::Comma)) {
            self.bump();
            let side_str = match self.bump() {
                Some((Token::Symbol(s), _)) => s,
                other => {
                    return Err(Diagnostic::error(
                        self.peek_span(),
                        format!("expected `U` or `L`, got {other:?}"),
                    ));
                }
            };
            match side_str.as_str() {
                "U" => Some(CaseSide::Upper),
                "L" => Some(CaseSide::Lower),
                _ => {
                    return Err(Diagnostic::error(
                        self.peek_span(),
                        format!("case-op side must be `U` or `L`, got {side_str:?}"),
                    ));
                }
            }
        } else {
            None
        };
        self.expect(&Token::RightParenthesis, "`)`")?;
        Ok(Self::spanned(
            PmatchExpr::CaseOp {
                op,
                side,
                body: Box::new(body),
            },
            self.merge(start),
        ))
    }

    fn parse_explode_implode(
        &mut self,
        explode: bool,
        start: usize,
    ) -> Result<SpannedExpr, Diagnostic> {
        self.bump(); // ExplodeLeft / ImplodeLeft
        // Body is a `CONCATENATED_STRING_LIST`: stringlikes separated by COMMA.
        let mut items = Vec::new();
        loop {
            let item = self.parse_expression13()?;
            items.push(item);
            if matches!(self.peek(), Some(Token::Comma)) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(&Token::RightParenthesis, "`)`")?;
        let value = if explode {
            PmatchExpr::Explode(items)
        } else {
            PmatchExpr::Implode(items)
        };
        Ok(Self::spanned(value, self.merge(start)))
    }

    fn parse_unary_call(
        &mut self,
        ctor: fn(Box<SpannedExpr>) -> PmatchExpr,
        start: usize,
    ) -> Result<SpannedExpr, Diagnostic> {
        self.bump(); // <op>Left
        let body = self.parse_expression2()?;
        self.expect(&Token::RightParenthesis, "`)`")?;
        Ok(Self::spanned(ctor(Box::new(body)), self.merge(start)))
    }

    fn parse_or_and(&mut self, is_or: bool, start: usize) -> Result<SpannedExpr, Diagnostic> {
        self.bump(); // OrLeft / AndLeft
        let mut items = Vec::new();
        loop {
            let item = self.parse_expression2()?;
            items.push(item);
            if matches!(self.peek(), Some(Token::Comma)) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(&Token::RightParenthesis, "`)`")?;
        let value = if is_or {
            PmatchExpr::OrContext(items)
        } else {
            PmatchExpr::AndContext(items)
        };
        Ok(Self::spanned(value, self.merge(start)))
    }

    fn parse_like(&mut self, unlike: bool, start: usize) -> Result<SpannedExpr, Diagnostic> {
        self.bump(); // LikeLeft / UnlikeLeft
        let args = self.parse_arglist()?;
        self.expect(&Token::RightParenthesis, "`)`")?;
        // Optional CATENATE_N tail.
        let threshold = if let Some(Token::CatenateN(_)) = self.peek() {
            let (tok, _) = self.bump().unwrap();
            match tok {
                Token::CatenateN(n) => Some(n),
                _ => None,
            }
        } else {
            None
        };
        Ok(Self::spanned(
            PmatchExpr::Like {
                args,
                threshold,
                unlike,
            },
            self.merge(start),
        ))
    }

    fn parse_interpolate(&mut self, start: usize) -> Result<SpannedExpr, Diagnostic> {
        self.bump(); // InterpolateLeft
        let mut items = Vec::new();
        if !matches!(self.peek(), Some(Token::RightParenthesis)) {
            loop {
                items.push(self.parse_expression2()?);
                if matches!(self.peek(), Some(Token::Comma)) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        self.expect(&Token::RightParenthesis, "`)`")?;
        Ok(Self::spanned(
            PmatchExpr::Interpolate(items),
            self.merge(start),
        ))
    }

    fn parse_uncompose(&mut self, start: usize) -> Result<SpannedExpr, Diagnostic> {
        self.bump(); // UncomposeLeft
        let a = self.parse_expression13()?;
        self.expect(&Token::Comma, "`,`")?;
        let b = self.parse_expression13()?;
        self.expect(&Token::Comma, "`,`")?;
        let c = self.parse_expression13()?;
        self.expect(&Token::RightParenthesis, "`)`")?;
        Ok(Self::spanned(
            PmatchExpr::Uncompose(Box::new(a), Box::new(b), Box::new(c)),
            self.merge(start),
        ))
    }

    fn parse_user_call(&mut self, start: usize) -> Result<SpannedExpr, Diagnostic> {
        let (tok, _) = self.bump().unwrap();
        let name = match tok {
            Token::SymbolWithLeftParen(s) => s,
            _ => unreachable!(),
        };
        let mut args = Vec::new();
        if !matches!(self.peek(), Some(Token::RightParenthesis)) {
            loop {
                args.push(self.parse_expression2()?);
                if matches!(self.peek(), Some(Token::Comma)) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        self.expect(&Token::RightParenthesis, "`)`")?;
        Ok(Self::spanned(
            PmatchExpr::Call { name, args },
            self.merge(start),
        ))
    }
}

impl MappingSide {
    /// Convert a parsed expression to a `MappingSide`, collapsing a top-level
    /// `BracketedDotted` into the dedicated `Dotted` variant.
    fn from_expr(expr: SpannedExpr) -> Self {
        match expr.value {
            PmatchExpr::BracketedDotted(inner) => MappingSide::Dotted(inner),
            other => MappingSide::Expr(Box::new(Spanned::new(other, expr.span))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(src: &str) -> PmatchFile {
        parse(src)
            .unwrap_or_else(|e| panic!("parse {src:?}: {e:?}"))
            .value
    }

    #[test]
    fn smallest_define() {
        let f = parsed(r#"Define TOP "foo";"#);
        assert_eq!(f.statements.len(), 1);
        match &f.statements[0].value {
            PmatchStatement::Define { name, params, body } => {
                assert_eq!(name, "TOP");
                assert!(params.is_none());
                assert!(matches!(&body.value, PmatchExpr::QuotedLiteral(s) if s == "foo"));
            }
            other => panic!("expected Define, got {other:?}"),
        }
    }

    #[test]
    fn endtag_call() {
        let f = parsed("Define TOP Alpha+ EndTag(Word);");
        match &f.statements[0].value {
            PmatchStatement::Define { body, .. } => match &body.value {
                PmatchExpr::Binary(BinaryOp::Concatenate, _, end) => {
                    assert!(matches!(&end.value, PmatchExpr::EndTag(s) if s == "Word"));
                }
                other => panic!("got {other:?}"),
            },
            _ => panic!(),
        }
    }

    #[test]
    fn function_definition() {
        let f = parsed("Define MyTag(name, body) [body EndTag(name)];");
        match &f.statements[0].value {
            PmatchStatement::Define { params, .. } => {
                assert_eq!(params.as_ref().unwrap(), &vec!["name", "body"]);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn set_variable() {
        let f = parsed("set need-separators off");
        assert!(matches!(
            &f.statements[0].value,
            PmatchStatement::SetVariable { name, .. } if name == "need-separators"
        ));
    }

    #[test]
    fn regex_top() {
        let f = parsed("regex Alpha+ EndTag(W);");
        assert!(matches!(
            &f.statements[0].value,
            PmatchStatement::RegexTop { .. }
        ));
    }

    #[test]
    fn list_definition() {
        let f = parsed("list animals {dog} | {cat};");
        assert!(matches!(
            &f.statements[0].value,
            PmatchStatement::ListDefinition { name, .. } if name == "animals"
        ));
    }

    #[test]
    fn tag_postfix() {
        let f = parsed("Define TOP [Alpha+].t(W);");
        match &f.statements[0].value {
            PmatchStatement::Define { body, .. } => {
                assert!(matches!(
                    &body.value,
                    PmatchExpr::Tag { name, .. } if name == "W"
                ));
            }
            _ => panic!(),
        }
    }
}
