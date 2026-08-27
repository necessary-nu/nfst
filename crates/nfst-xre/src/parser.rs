//! Recursive-descent parser for xre, faithful to the precedence ladder in
//! `xre_parse.yy` (REGEXP1 → REGEXP12). The grammar is small enough that a
//! direct parser is the right shape — chumsky stays in the workspace for the
//! larger languages where its error-recovery story will be load-bearing.
//!
//! Every node returned by the parser is `Spanned<XreExpr>`; spans are computed
//! from the first consumed token through the last consumed token of each
//! production.

use crate::ast::{
    BinaryOp, ContextMark, MappingKind, MappingPair, MappingSide, ReadKind, ReplaceArrow,
    ReplaceContext, ReplaceContexts, ReplaceRule, RestrContext, SpannedXre, SubstituteWhat,
    UnaryOp, XreExpr,
};
use crate::lexer::{LexError, tokenize};
use crate::token::Token;
use nfst_syntax::{Diagnostic, Span, Spanned};
use smol_str::SmolStr;

#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse(source: &str) -> Result<SpannedXre, ParseError> {
    let tokens = tokenize(source).map_err(|errs| ParseError {
        diagnostics: errs.into_iter().map(lex_error_to_diag).collect(),
    })?;
    let mut p = Parser::new(tokens);
    let expr = p.parse_regexp1().map_err(|d| ParseError {
        diagnostics: vec![d],
    })?;
    if !p.is_at_end() {
        return Err(ParseError {
            diagnostics: vec![Diagnostic::error(
                p.peek_span(),
                format!("unexpected trailing input near {:?}", p.peek()),
            )],
        });
    }
    Ok(expr)
}

/// Parse a file that may contain multiple semicolon-terminated expressions.
/// Empty files (or files containing only comments) return `Ok(vec![])`.
pub fn parse_all(source: &str) -> Result<Vec<SpannedXre>, ParseError> {
    let tokens = tokenize(source).map_err(|errs| ParseError {
        diagnostics: errs.into_iter().map(lex_error_to_diag).collect(),
    })?;
    let mut p = Parser::new(tokens);
    let mut out = Vec::new();
    while !p.is_at_end() {
        let expr = p.parse_regexp1().map_err(|d| ParseError {
            diagnostics: vec![d],
        })?;
        out.push(expr);
    }
    Ok(out)
}

fn lex_error_to_diag(e: LexError) -> Diagnostic {
    Diagnostic::error(e.span, format!("lex error: {:?} at {:?}", e.kind, e.slice))
}

// ───────────────────────── parser core ─────────────────────────

struct Parser {
    tokens: Vec<(Token, Span)>,
    pos: usize,
    /// End offset of the most-recently-consumed token, used to close out spans
    /// when the lookahead has not yet advanced past the production's last
    /// token.
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

    fn expect(&mut self, expected: &Token) -> Result<Span, Diagnostic> {
        match self.peek() {
            Some(t) if std::mem::discriminant(t) == std::mem::discriminant(expected) => {
                Ok(self.bump().unwrap().1)
            }
            _ => Err(self.err(format!("expected {expected:?}"))),
        }
    }

    fn spanned(value: XreExpr, span: Span) -> SpannedXre {
        Spanned::new(value, span)
    }

    fn merge(&self, start: usize) -> Span {
        Span::anonymous(start..self.last_end)
    }

    // ───────────────────────── regexp1 ─────────────────────────
    // XRE: REGEXP1 | (only comments)
    // REGEXP1: REGEXP2 [;]
    fn parse_regexp1(&mut self) -> Result<SpannedXre, Diagnostic> {
        let expr = self.parse_regexp2()?;
        if matches!(self.peek(), Some(Token::EndOfExpression)) {
            self.bump();
        }
        Ok(expr)
    }

    // ───────────────────────── regexp2 ─────────────────────────
    // Composition family + substitute. Substitute starts with `.
    fn parse_regexp2(&mut self) -> Result<SpannedXre, Diagnostic> {
        if matches!(self.peek(), Some(Token::SubstituteLeft)) {
            return self.parse_substitute();
        }

        let start = self.current_start();
        let mut left = self.parse_replace()?;
        loop {
            let op = match self.peek() {
                Some(Token::Composition) => Some(BinaryOp::Compose),
                Some(Token::CrossProduct) => Some(BinaryOp::CrossProduct),
                Some(Token::LenientComposition) => Some(BinaryOp::LenientCompose),
                Some(Token::MergeRightArrow) => Some(BinaryOp::MergeRight),
                Some(Token::MergeLeftArrow) => Some(BinaryOp::MergeLeft),
                _ => None,
            };
            let Some(op) = op else { break };
            self.bump();
            let right = self.parse_replace()?;
            left = Self::spanned(
                XreExpr::Binary(op, Box::new(left), Box::new(right)),
                self.merge(start),
            );
        }
        Ok(left)
    }

    // ───────────────────────── replace ─────────────────────────
    // REPLACE: REGEXP3 | PARALLEL_RULES
    //
    // We parse a regexp3 (or a dotted-bracket group, which can be the empty
    // `[..]` form) speculatively, then if a replace arrow follows, treat the
    // parsed expression as the upper side of a mapping pair and continue
    // building the parallel rule list. Markup forms (`-> X ... Y`, `-> X ...`,
    // `-> ... Y`) and dotted-bracket sides are recognized in the rhs.
    fn parse_replace(&mut self) -> Result<SpannedXre, Diagnostic> {
        let start = self.current_start();
        let first = self.parse_regexp3()?;

        if let Some(arrow) = self.peek_replace_arrow() {
            self.bump();
            let mapping = self.parse_mapping_after_arrow(first, arrow)?;

            // additional mapping pairs separated by COMMA in the same arrow
            let mut mappings = vec![mapping];
            while matches!(self.peek(), Some(Token::Comma)) && self.peek_starts_mapping_lhs_at(1) {
                self.bump(); // ,
                let upper = self.parse_mapping_lhs()?;
                // Each mapping keeps its own arrow: regex.y imposes no
                // agreement across a parallel list.
                let arrow2 = self.expect_replace_arrow()?;
                mappings.push(self.parse_mapping_after_arrow_with_upper(upper, arrow2)?);
            }

            let contexts = self.try_parse_replace_contexts()?;
            let mut rules = vec![ReplaceRule { mappings, contexts }];

            while matches!(self.peek(), Some(Token::Commacomma)) {
                self.bump();
                let extra = self.parse_replace_rule()?;
                rules.push(extra);
            }

            return Ok(Self::spanned(
                XreExpr::Replace { arrow, rules },
                self.merge(start),
            ));
        }

        Ok(first)
    }

    /// One rule: one or more `MAPPINGPAIR` joined by COMMA, optional contexts.
    /// Caller has already consumed any leading `,,`.
    fn parse_replace_rule(&mut self) -> Result<ReplaceRule, Diagnostic> {
        let upper = self.parse_mapping_lhs()?;
        let arrow = self.expect_replace_arrow()?;
        let mut mappings = vec![self.parse_mapping_after_arrow_with_upper(upper, arrow)?];
        while matches!(self.peek(), Some(Token::Comma)) && self.peek_starts_mapping_lhs_at(1) {
            self.bump();
            let u = self.parse_mapping_lhs()?;
            let a2 = self.expect_replace_arrow()?;
            mappings.push(self.parse_mapping_after_arrow_with_upper(u, a2)?);
        }
        let contexts = self.try_parse_replace_contexts()?;
        Ok(ReplaceRule { mappings, contexts })
    }

    /// Construct a `MappingPair` given an already-parsed `upper` expression
    /// (which arrived here as an `SpannedXre` from a `parse_regexp3()`). The
    /// arrow has been consumed; this method handles the rhs and any markup
    /// tail.
    fn parse_mapping_after_arrow(
        &mut self,
        upper: SpannedXre,
        arrow: ReplaceArrow,
    ) -> Result<MappingPair, Diagnostic> {
        self.parse_mapping_after_arrow_with_upper(MappingSide::from_expr(upper), arrow)
    }

    fn parse_mapping_after_arrow_with_upper(
        &mut self,
        upper: MappingSide,
        arrow: ReplaceArrow,
    ) -> Result<MappingPair, Diagnostic> {
        // The four MAPPINGPAIR rhs shapes from xre_parse.yy:
        //   E                 — plain replacement
        //   E ... E           — markup with both pre and post
        //   E ...             — markup pre only
        //   ... E             — markup post only
        //
        // `[..]` and `[. E .]` arrive here as `BracketedDotted(...)` from
        // regexp11; the `MappingSide::from_expr` collapse turns those into
        // `MappingSide::Dotted(_)`. Plain replacements thus support the
        // dotted forms naturally. The MARKUP_MARKER (`...`) is the only
        // explicit decision point.

        if matches!(self.peek(), Some(Token::MarkupMarker)) {
            // ... E
            self.bump();
            let post = self.parse_mapping_side()?;
            return Ok(MappingPair {
                upper,
                arrow,
                kind: MappingKind::Markup {
                    pre: None,
                    post: Some(post),
                },
            });
        }

        let first_rhs = self.parse_mapping_side()?;

        if matches!(self.peek(), Some(Token::MarkupMarker)) {
            // E ... [E]
            self.bump();
            let post = if self.peek_starts_mapping_side() {
                Some(self.parse_mapping_side()?)
            } else {
                None
            };
            return Ok(MappingPair {
                upper,
                arrow,
                kind: MappingKind::Markup {
                    pre: Some(first_rhs),
                    post,
                },
            });
        }

        Ok(MappingPair {
            upper,
            arrow,
            kind: MappingKind::Plain { lower: first_rhs },
        })
    }

    fn parse_mapping_lhs(&mut self) -> Result<MappingSide, Diagnostic> {
        // The LHS of a mapping pair is one regexp3 or one dotted-bracket
        // group; we let regexp3 handle the dotted-bracket case via
        // BracketedDotted.
        let expr = self.parse_regexp3()?;
        Ok(MappingSide::from_expr(expr))
    }

    fn parse_mapping_side(&mut self) -> Result<MappingSide, Diagnostic> {
        let expr = self.parse_regexp3()?;
        Ok(MappingSide::from_expr(expr))
    }

    fn peek_starts_mapping_side(&self) -> bool {
        self.peek_starts_replace()
    }

    /// Look one token ahead from `self.pos + offset` and decide whether it
    /// could begin a mapping LHS. Used to disambiguate
    /// "a -> b , c -> d" (the `,` joins another mapping) from
    /// "a -> b , c _ d" inside a context list.
    fn peek_starts_mapping_lhs_at(&self, offset: usize) -> bool {
        let Some((tok, _)) = self.tokens.get(self.pos + offset) else {
            return false;
        };
        matches!(
            tok,
            Token::Symbol(_)
                | Token::MultiCharSymbol(_)
                | Token::QuotedLiteral(_)
                | Token::CurlyBrackets(_)
                | Token::EpsilonToken
                | Token::AnyToken
                | Token::LeftBracket
                | Token::LeftParenthesis
                | Token::LeftBracketDotted
                | Token::Complement
                | Token::TermComplement
                | Token::Containment
                | Token::ContainmentOnce
                | Token::ContainmentOpt
                | Token::ReadBin(_)
                | Token::ReadText(_)
                | Token::ReadSpaced(_)
                | Token::ReadProlog(_)
                | Token::ReadRe(_)
                | Token::FunctionName(_)
        )
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
            None => Err(self.err("expected a replace arrow (->, <-, <->, etc.)")),
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
        // CONTEXT: REPLACE _ REPLACE | REPLACE _ | _ REPLACE | _
        if matches!(self.peek(), Some(Token::CenterMarker)) {
            self.bump();
            if self.peek_starts_replace() {
                let r = self.parse_replace()?;
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
        let left = self.parse_replace()?;
        self.expect(&Token::CenterMarker)?;
        if self.peek_starts_replace() {
            let r = self.parse_replace()?;
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

    fn peek_starts_replace(&self) -> bool {
        self.peek_starts_mapping_lhs_at(0)
    }

    // ───────────────────────── regexp3 ─────────────────────────
    // before / after / shuffle (left-assoc)
    fn parse_regexp3(&mut self) -> Result<SpannedXre, Diagnostic> {
        let start = self.current_start();
        let mut left = self.parse_regexp4()?;
        loop {
            let op = match self.peek() {
                Some(Token::Before) => BinaryOp::Before,
                Some(Token::After) => BinaryOp::After,
                Some(Token::Shuffle) => BinaryOp::Shuffle,
                _ => break,
            };
            self.bump();
            let right = self.parse_regexp4()?;
            left = Self::spanned(
                XreExpr::Binary(op, Box::new(left), Box::new(right)),
                self.merge(start),
            );
        }
        Ok(left)
    }

    // ───────────────────────── regexp4 ─────────────────────────
    // restriction: REGEXP5 (=> ctx[, ctx]*)?
    fn parse_regexp4(&mut self) -> Result<SpannedXre, Diagnostic> {
        let start = self.current_start();
        let body = self.parse_regexp5()?;
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
                XreExpr::Restriction {
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
            if self.peek_starts_replace() {
                let r = self.parse_regexp4()?;
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
        let left = self.parse_regexp4()?;
        self.expect(&Token::CenterMarker)?;
        if self.peek_starts_replace() {
            let r = self.parse_regexp4()?;
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

    // ───────────────────────── regexp5 ─────────────────────────
    // union family: |, &, -, .-u., .-l., .P., .p.
    fn parse_regexp5(&mut self) -> Result<SpannedXre, Diagnostic> {
        let start = self.current_start();
        let mut left = self.parse_regexp6()?;
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
            let right = self.parse_regexp6()?;
            left = Self::spanned(
                XreExpr::Binary(op, Box::new(left), Box::new(right)),
                self.merge(start),
            );
        }
        Ok(left)
    }

    // ───────────────────────── regexp6 ─────────────────────────
    // concatenation by juxtaposition (no operator)
    fn parse_regexp6(&mut self) -> Result<SpannedXre, Diagnostic> {
        let start = self.current_start();
        let mut left = self.parse_regexp7()?;
        while self.peek_starts_atom() {
            let right = self.parse_regexp7()?;
            left = Self::spanned(
                XreExpr::Binary(BinaryOp::Concatenate, Box::new(left), Box::new(right)),
                self.merge(start),
            );
        }
        Ok(left)
    }

    fn peek_starts_atom(&self) -> bool {
        // anything that can begin a regexp7/8/9/10/11/12. Operators that take
        // an expression on their right (~, $, $., $?, \) also start regexp8.
        self.peek_starts_replace()
    }

    // ───────────────────────── regexp7 ─────────────────────────
    // ignoring family: /, ./., \\\
    fn parse_regexp7(&mut self) -> Result<SpannedXre, Diagnostic> {
        let start = self.current_start();
        let mut left = self.parse_regexp8()?;
        loop {
            let op = match self.peek() {
                Some(Token::Ignoring) => BinaryOp::Ignoring,
                Some(Token::IgnoreInternally) => BinaryOp::IgnoreInternally,
                Some(Token::LeftQuotient) => BinaryOp::LeftQuotient,
                _ => break,
            };
            self.bump();
            let right = self.parse_regexp8()?;
            left = Self::spanned(
                XreExpr::Binary(op, Box::new(left), Box::new(right)),
                self.merge(start),
            );
        }
        Ok(left)
    }

    // ───────────────────────── regexp8 ─────────────────────────
    // unary prefixes: ~, $, $., $?, $::w
    fn parse_regexp8(&mut self) -> Result<SpannedXre, Diagnostic> {
        let start = self.current_start();
        match self.peek() {
            Some(Token::Complement) => {
                self.bump();
                let inner = self.parse_regexp8()?;
                Ok(Self::spanned(
                    XreExpr::Unary(UnaryOp::Complement, Box::new(inner)),
                    self.merge(start),
                ))
            }
            Some(Token::Containment) => {
                self.bump();
                if let Some(Token::Weight(_)) = self.peek() {
                    let weight = match self.bump().unwrap().0 {
                        Token::Weight(w) => w,
                        _ => unreachable!(),
                    };
                    let inner = self.parse_regexp8()?;
                    Ok(Self::spanned(
                        XreExpr::ContainmentWithWeight {
                            expr: Box::new(inner),
                            weight,
                        },
                        self.merge(start),
                    ))
                } else {
                    let inner = self.parse_regexp8()?;
                    Ok(Self::spanned(
                        XreExpr::Unary(UnaryOp::Containment, Box::new(inner)),
                        self.merge(start),
                    ))
                }
            }
            Some(Token::ContainmentOnce) => {
                self.bump();
                let inner = self.parse_regexp8()?;
                Ok(Self::spanned(
                    XreExpr::Unary(UnaryOp::ContainmentOnce, Box::new(inner)),
                    self.merge(start),
                ))
            }
            Some(Token::ContainmentOpt) => {
                self.bump();
                let inner = self.parse_regexp8()?;
                Ok(Self::spanned(
                    XreExpr::Unary(UnaryOp::ContainmentOpt, Box::new(inner)),
                    self.merge(start),
                ))
            }
            _ => self.parse_regexp9(),
        }
    }

    // ───────────────────────── regexp9 ─────────────────────────
    // postfix unaries: *, +, .r, .i, .u, .l, ^N, ^>N, ^<N, ^N,K
    fn parse_regexp9(&mut self) -> Result<SpannedXre, Diagnostic> {
        let start = self.current_start();
        let mut expr = self.parse_regexp10()?;
        loop {
            let next = self.peek().cloned();
            expr = match next {
                Some(Token::Star) => {
                    self.bump();
                    Self::spanned(
                        XreExpr::Unary(UnaryOp::Star, Box::new(expr)),
                        self.merge(start),
                    )
                }
                Some(Token::Plus) => {
                    self.bump();
                    Self::spanned(
                        XreExpr::Unary(UnaryOp::Plus, Box::new(expr)),
                        self.merge(start),
                    )
                }
                Some(Token::Reverse) => {
                    self.bump();
                    Self::spanned(
                        XreExpr::Unary(UnaryOp::Reverse, Box::new(expr)),
                        self.merge(start),
                    )
                }
                Some(Token::Invert) => {
                    self.bump();
                    Self::spanned(
                        XreExpr::Unary(UnaryOp::Invert, Box::new(expr)),
                        self.merge(start),
                    )
                }
                Some(Token::XreUpper) => {
                    self.bump();
                    Self::spanned(
                        XreExpr::Unary(UnaryOp::UpperProject, Box::new(expr)),
                        self.merge(start),
                    )
                }
                Some(Token::XreLower) => {
                    self.bump();
                    Self::spanned(
                        XreExpr::Unary(UnaryOp::LowerProject, Box::new(expr)),
                        self.merge(start),
                    )
                }
                Some(Token::CatenateN(_))
                | Some(Token::CatenateNPlus(_))
                | Some(Token::CatenateNMinus(_))
                | Some(Token::CatenateNToK(_, _)) => {
                    let (tok, _) = self.bump().unwrap();
                    let span = self.merge(start);
                    let value = match tok {
                        Token::CatenateN(n) => XreExpr::RepeatN(Box::new(expr), n),
                        Token::CatenateNPlus(n) => XreExpr::RepeatNPlus(Box::new(expr), n),
                        Token::CatenateNMinus(n) => XreExpr::RepeatNMinus(Box::new(expr), n),
                        Token::CatenateNToK(n, k) => XreExpr::RepeatNToK(Box::new(expr), n, k),
                        _ => unreachable!(),
                    };
                    Self::spanned(value, span)
                }
                _ => break,
            };
        }
        Ok(expr)
    }

    // ───────────────────────── regexp10 ─────────────────────────
    // term complement prefix: \E
    fn parse_regexp10(&mut self) -> Result<SpannedXre, Diagnostic> {
        let start = self.current_start();
        if matches!(self.peek(), Some(Token::TermComplement)) {
            self.bump();
            let inner = self.parse_regexp10()?;
            return Ok(Self::spanned(
                XreExpr::Unary(UnaryOp::TermComplement, Box::new(inner)),
                self.merge(start),
            ));
        }
        self.parse_regexp11()
    }

    // ───────────────────────── regexp11 ─────────────────────────
    // bracketed / parenthesized / dotted / pair forms
    fn parse_regexp11(&mut self) -> Result<SpannedXre, Diagnostic> {
        let start = self.current_start();
        match self.peek() {
            Some(Token::LeftBracket) => {
                self.bump();
                let inner = self.parse_regexp2()?;
                self.expect(&Token::RightBracket)?;
                let group = Self::spanned(XreExpr::Group(Box::new(inner)), self.merge(start));
                self.parse_optional_pair_or_weight(group, start)
            }
            Some(Token::LeftParenthesis) => {
                self.bump();
                let inner = self.parse_regexp2()?;
                self.expect(&Token::RightParenthesis)?;
                Ok(Self::spanned(
                    XreExpr::Optional(Box::new(inner)),
                    self.merge(start),
                ))
            }
            Some(Token::LeftBracketDotted) => {
                self.bump();
                if matches!(self.peek(), Some(Token::RightBracketDotted)) {
                    self.bump();
                    Ok(Self::spanned(
                        XreExpr::BracketedDotted(None),
                        self.merge(start),
                    ))
                } else {
                    let inner = self.parse_regexp2()?;
                    self.expect(&Token::RightBracketDotted)?;
                    Ok(Self::spanned(
                        XreExpr::BracketedDotted(Some(Box::new(inner))),
                        self.merge(start),
                    ))
                }
            }
            _ => self.parse_regexp12(),
        }
    }

    /// After a `[ E ]` group, allow `:` (pair) or `::w` (weight) postfix.
    fn parse_optional_pair_or_weight(
        &mut self,
        lhs: SpannedXre,
        start: usize,
    ) -> Result<SpannedXre, Diagnostic> {
        if matches!(self.peek(), Some(Token::PairSeparator)) {
            self.bump();
            let rhs = self.parse_pair_rhs()?;
            return Ok(Self::spanned(
                XreExpr::Pair {
                    upper: Box::new(lhs),
                    lower: Box::new(rhs),
                },
                self.merge(start),
            ));
        }
        if let Some(Token::Weight(_)) = self.peek() {
            let weight = match self.bump().unwrap().0 {
                Token::Weight(w) => w,
                _ => unreachable!(),
            };
            return Ok(Self::spanned(
                XreExpr::Weighted {
                    expr: Box::new(lhs),
                    weight,
                },
                self.merge(start),
            ));
        }
        Ok(lhs)
    }

    fn parse_pair_rhs(&mut self) -> Result<SpannedXre, Diagnostic> {
        let start = self.current_start();
        match self.peek() {
            Some(Token::LeftBracket) => {
                self.bump();
                let inner = self.parse_regexp2()?;
                self.expect(&Token::RightBracket)?;
                Ok(Self::spanned(
                    XreExpr::Group(Box::new(inner)),
                    self.merge(start),
                ))
            }
            Some(Token::CurlyBrackets(_)) => {
                let s = match self.bump().unwrap().0 {
                    Token::CurlyBrackets(s) => s,
                    _ => unreachable!(),
                };
                Ok(Self::spanned(XreExpr::Curly(s), self.merge(start)))
            }
            _ => self.parse_halfarc(),
        }
    }

    // ───────────────────────── regexp12 ─────────────────────────
    // labels / weighted label / file loads / function call
    fn parse_regexp12(&mut self) -> Result<SpannedXre, Diagnostic> {
        let start = self.current_start();
        match self.peek() {
            Some(Token::ReadBin(_))
            | Some(Token::ReadText(_))
            | Some(Token::ReadSpaced(_))
            | Some(Token::ReadProlog(_))
            | Some(Token::ReadRe(_)) => {
                let (tok, _) = self.bump().unwrap();
                let (kind, path) = match tok {
                    Token::ReadBin(p) => (ReadKind::Binary, p),
                    Token::ReadText(p) => (ReadKind::Text, p),
                    Token::ReadSpaced(p) => (ReadKind::Spaced, p),
                    Token::ReadProlog(p) => (ReadKind::Prolog, p),
                    Token::ReadRe(p) => (ReadKind::Regex, p),
                    _ => unreachable!(),
                };
                Ok(Self::spanned(
                    XreExpr::ReadFile { kind, path },
                    self.merge(start),
                ))
            }
            Some(Token::FunctionName(_)) => self.parse_function_call(start),
            _ => {
                let label = self.parse_label()?;
                if let Some(Token::Weight(_)) = self.peek() {
                    let weight = match self.bump().unwrap().0 {
                        Token::Weight(w) => w,
                        _ => unreachable!(),
                    };
                    Ok(Self::spanned(
                        XreExpr::Weighted {
                            expr: Box::new(label),
                            weight,
                        },
                        self.merge(start),
                    ))
                } else {
                    Ok(label)
                }
            }
        }
    }

    fn parse_label(&mut self) -> Result<SpannedXre, Diagnostic> {
        let start = self.current_start();
        if let Some(Token::CurlyBrackets(_)) = self.peek() {
            let upper = match self.bump().unwrap().0 {
                Token::CurlyBrackets(s) => Self::spanned(XreExpr::Curly(s), self.merge(start)),
                _ => unreachable!(),
            };
            if matches!(self.peek(), Some(Token::PairSeparator)) {
                self.bump();
                let lower = self.parse_pair_rhs()?;
                return Ok(Self::spanned(
                    XreExpr::Pair {
                        upper: Box::new(upper),
                        lower: Box::new(lower),
                    },
                    self.merge(start),
                ));
            }
            return Ok(upper);
        }

        let upper = self.parse_halfarc()?;

        if matches!(self.peek(), Some(Token::PairSeparator)) {
            self.bump();
            let lower = self.parse_pair_rhs()?;
            return Ok(Self::spanned(
                XreExpr::Pair {
                    upper: Box::new(upper),
                    lower: Box::new(lower),
                },
                self.merge(start),
            ));
        }

        Ok(upper)
    }

    fn parse_halfarc(&mut self) -> Result<SpannedXre, Diagnostic> {
        let start = self.current_start();
        match self.peek() {
            Some(Token::Symbol(_))
            | Some(Token::MultiCharSymbol(_))
            | Some(Token::QuotedLiteral(_)) => {
                let (tok, _) = self.bump().unwrap();
                let value = match tok {
                    Token::Symbol(s) | Token::MultiCharSymbol(s) | Token::QuotedLiteral(s) => {
                        XreExpr::Symbol(s)
                    }
                    _ => unreachable!(),
                };
                Ok(Self::spanned(value, self.merge(start)))
            }
            Some(Token::EpsilonToken) => {
                self.bump();
                Ok(Self::spanned(XreExpr::Epsilon, self.merge(start)))
            }
            Some(Token::AnyToken) => {
                self.bump();
                Ok(Self::spanned(XreExpr::Any, self.merge(start)))
            }
            _ => Err(self.err(format!(
                "expected a label (symbol, quoted literal, epsilon, or `?`), got {:?}",
                self.peek()
            ))),
        }
    }

    fn parse_halfarc_string(&mut self) -> Result<SmolStr, Diagnostic> {
        match self.peek() {
            Some(Token::Symbol(_))
            | Some(Token::MultiCharSymbol(_))
            | Some(Token::QuotedLiteral(_)) => {
                let (tok, _) = self.bump().unwrap();
                Ok(match tok {
                    Token::Symbol(s) | Token::MultiCharSymbol(s) | Token::QuotedLiteral(s) => s,
                    _ => unreachable!(),
                })
            }
            Some(Token::EpsilonToken) => {
                self.bump();
                Ok("@_EPSILON_SYMBOL_@".into())
            }
            Some(Token::AnyToken) => {
                self.bump();
                Ok("@_UNKNOWN_SYMBOL_@".into())
            }
            _ => Err(self.err("expected a halfarc symbol")),
        }
    }

    fn parse_function_call(&mut self, start: usize) -> Result<SpannedXre, Diagnostic> {
        let name = match self.bump().unwrap().0 {
            Token::FunctionName(s) => s.trim_end_matches('(').into(),
            _ => unreachable!(),
        };
        let mut args = Vec::new();
        if !matches!(self.peek(), Some(Token::RightParenthesis)) {
            loop {
                args.push(self.parse_regexp2()?);
                if matches!(self.peek(), Some(Token::Comma)) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        self.expect(&Token::RightParenthesis)?;
        Ok(Self::spanned(
            XreExpr::FunctionCall { name, args },
            self.merge(start),
        ))
    }

    // ───────────────────────── substitute ─────────────────────────
    fn parse_substitute(&mut self) -> Result<SpannedXre, Diagnostic> {
        let start = self.current_start();
        self.expect(&Token::SubstituteLeft)?;
        self.expect(&Token::LeftBracket)?;
        let haystack = self.parse_replace()?;
        self.expect(&Token::Comma)?;

        let first = self.parse_halfarc_string()?;

        if matches!(self.peek(), Some(Token::PairSeparator)) {
            self.bump();
            let from_lower = self.parse_halfarc_string()?;
            self.expect(&Token::Comma)?;
            let to_upper = self.parse_halfarc_string()?;
            self.expect(&Token::PairSeparator)?;
            let to_lower = self.parse_halfarc_string()?;
            self.expect(&Token::RightBracket)?;
            return Ok(Self::spanned(
                XreExpr::Substitute {
                    haystack: Box::new(haystack),
                    what: SubstituteWhat::Pair {
                        from: (first, from_lower),
                        to: (to_upper, to_lower),
                    },
                },
                self.merge(start),
            ));
        }

        // symbol-substitute: first is the needle; the rest until ] is the list.
        self.expect(&Token::Comma)?;
        let mut replacement = Vec::new();
        while !matches!(self.peek(), Some(Token::RightBracket)) {
            replacement.push(self.parse_halfarc_string()?);
        }
        self.expect(&Token::RightBracket)?;
        Ok(Self::spanned(
            XreExpr::Substitute {
                haystack: Box::new(haystack),
                what: SubstituteWhat::Symbol {
                    needle: first,
                    replacement,
                },
            },
            self.merge(start),
        ))
    }
}

impl MappingSide {
    /// Convert a parsed expression to a `MappingSide`, collapsing a top-level
    /// `BracketedDotted` into the dedicated `Dotted` variant so consumers
    /// can distinguish `[. E .] -> X` from `E -> X`.
    fn from_expr(expr: SpannedXre) -> Self {
        match expr.value {
            XreExpr::BracketedDotted(inner) => MappingSide::Dotted(inner),
            other => MappingSide::Expr(Box::new(Spanned::new(other, expr.span))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(src: &str) -> XreExpr {
        parse(src)
            .unwrap_or_else(|e| panic!("parse failed: {:?}", e))
            .value
    }

    fn sym(s: &str) -> XreExpr {
        XreExpr::Symbol(s.into())
    }

    fn s(value: XreExpr) -> SpannedXre {
        Spanned::new(value, Span::anonymous(0..0))
    }

    fn b(value: XreExpr) -> Box<SpannedXre> {
        Box::new(s(value))
    }

    #[test]
    fn single_symbol() {
        assert_eq!(p("a"), sym("a"));
    }

    #[test]
    fn concatenation() {
        // "c a t" → ((c · a) · t)
        let expected = XreExpr::Binary(
            BinaryOp::Concatenate,
            b(XreExpr::Binary(
                BinaryOp::Concatenate,
                b(sym("c")),
                b(sym("a")),
            )),
            b(sym("t")),
        );
        assert_eq!(p("c a t"), expected);
    }

    #[test]
    fn union() {
        let expected = XreExpr::Binary(BinaryOp::Union, b(sym("a")), b(sym("b")));
        assert_eq!(p("a | b"), expected);
    }

    #[test]
    fn pair() {
        let expected = XreExpr::Pair {
            upper: b(sym("a")),
            lower: b(sym("b")),
        };
        assert_eq!(p("a:b"), expected);
    }

    #[test]
    fn weight_on_label() {
        let expected = XreExpr::Weighted {
            expr: b(sym("a")),
            weight: 1.5,
        };
        assert_eq!(p("a::1.5"), expected);
    }

    #[test]
    fn star_postfix() {
        let expected = XreExpr::Unary(UnaryOp::Star, b(sym("a")));
        assert_eq!(p("a*"), expected);
    }

    #[test]
    fn complement_containment_brackets() {
        let inner = XreExpr::Group(b(sym("a")));
        let containment = XreExpr::Unary(UnaryOp::Containment, Box::new(s(inner)));
        let complement = XreExpr::Unary(UnaryOp::Complement, Box::new(s(containment)));
        assert_eq!(p("~$[ a ]"), complement);
    }

    #[test]
    fn left_arrow_replace() {
        let m = MappingPair {
            upper: MappingSide::Expr(b(XreExpr::Epsilon)),
            arrow: ReplaceArrow::Left,
            kind: MappingKind::Plain {
                lower: MappingSide::Expr(b(sym("a"))),
            },
        };
        let expected = XreExpr::Replace {
            arrow: ReplaceArrow::Left,
            rules: vec![ReplaceRule {
                mappings: vec![m],
                contexts: None,
            }],
        };
        assert_eq!(p("0 <- a"), expected);
    }

    #[test]
    fn parallel_left_arrow() {
        let result = p("0 <- a , 0 <- b , 0 <- c");
        match result {
            XreExpr::Replace { arrow, rules } => {
                assert_eq!(arrow, ReplaceArrow::Left);
                assert_eq!(rules.len(), 1);
                assert_eq!(rules[0].mappings.len(), 3);
            }
            other => panic!("expected Replace, got {other:?}"),
        }
    }

    #[test]
    fn semicolon_terminated_okay() {
        assert_eq!(p("a ;"), sym("a"));
        assert_eq!(p("a;"), sym("a"));
    }

    #[test]
    fn multi_expression_parse_all() {
        let exprs: Vec<_> = parse_all("a; b; c")
            .unwrap()
            .into_iter()
            .map(|e| e.value)
            .collect();
        assert_eq!(exprs, vec![sym("a"), sym("b"), sym("c")]);
    }

    #[test]
    fn comments_only_yields_empty_via_parse_all() {
        let exprs = parse_all("! hello\n# world").unwrap();
        assert!(exprs.is_empty());
    }

    #[test]
    fn read_file_atom() {
        let expr = p(r#"@"cat.foma" "." @"cat.foma""#);
        match &expr {
            XreExpr::Binary(BinaryOp::Concatenate, _, _) => {}
            other => panic!("expected concat at top, got {other:?}"),
        }
    }

    #[test]
    fn span_covers_whole_top_expression() {
        let result = parse("a b c").unwrap();
        // span starts at byte 0, covers through 'c' (offset 4).
        assert_eq!(result.span.start(), 0);
        assert_eq!(result.span.end(), 5);
    }

    #[test]
    fn span_on_unary_includes_operator() {
        let result = parse("a*").unwrap();
        assert_eq!(result.span.start(), 0);
        assert_eq!(result.span.end(), 2);
    }

    #[test]
    fn span_on_inner_node_is_inner_only() {
        // For `a*`, the inner Symbol("a") span should cover just byte 0..1.
        let result = parse("a*").unwrap();
        if let XreExpr::Unary(_, inner) = &result.value {
            assert_eq!(inner.span.start(), 0);
            assert_eq!(inner.span.end(), 1);
        } else {
            panic!("expected Unary, got {:?}", result.value);
        }
    }
}
