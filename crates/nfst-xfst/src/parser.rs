//! Recursive-descent xfst parser. Dispatches each command on its
//! [`Token::Command`] kind. Embedded `regex`/`define` bodies are passed
//! to [`nfst_xre::parse`].

use crate::ast::{
    ApplyKind, NetworkOp, PrintCmd, ReadCmd, Redirect, RedirectKind, SaveCmd, SubstituteCmd,
    TestKind, XfstCommand, XfstScript,
};
use crate::lexer::{LexError, tokenize};
use crate::token::{CommandKind, Token};
use nfst_syntax::{Diagnostic, Span, Spanned};
use nfst_xre::SpannedXre;

#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse(source: &str) -> Result<Spanned<XfstScript>, ParseError> {
    let tokens = tokenize(source).map_err(|errs| ParseError {
        diagnostics: errs.into_iter().map(lex_error_to_diag).collect(),
    })?;
    let mut p = Parser::new(tokens);
    p.parse_script().map_err(|d| ParseError {
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

    /// Consume an optional `;` (or several).
    fn eat_semicolons(&mut self) {
        while matches!(self.peek(), Some(Token::Semicolon)) {
            self.bump();
        }
    }

    fn parse_script(&mut self) -> Result<Spanned<XfstScript>, Diagnostic> {
        let start = self.current_start();
        let mut commands = Vec::new();
        self.eat_semicolons();
        while !self.is_at_end() {
            let cmd = self.parse_command()?;
            commands.push(cmd);
            self.eat_semicolons();
        }
        Ok(Spanned::new(XfstScript { commands }, self.merge(start)))
    }

    fn parse_command(&mut self) -> Result<Spanned<XfstCommand>, Diagnostic> {
        let start = self.current_start();
        let cmd = self.parse_command_inner(start)?;
        // Tail handling: optional redirect.
        if let Some((tok, _)) = self.tokens.get(self.pos).cloned() {
            match tok {
                Token::RedirectOut(p) => {
                    self.bump();
                    let redirect = Redirect {
                        kind: RedirectKind::Out,
                        path: p,
                    };
                    return Ok(Spanned::new(
                        XfstCommand::Redirected {
                            command: Box::new(cmd),
                            redirect,
                        },
                        self.merge(start),
                    ));
                }
                Token::RedirectAppend(p) => {
                    self.bump();
                    let redirect = Redirect {
                        kind: RedirectKind::Append,
                        path: p,
                    };
                    return Ok(Spanned::new(
                        XfstCommand::Redirected {
                            command: Box::new(cmd),
                            redirect,
                        },
                        self.merge(start),
                    ));
                }
                Token::RedirectIn(p) => {
                    self.bump();
                    let redirect = Redirect {
                        kind: RedirectKind::In,
                        path: p,
                    };
                    return Ok(Spanned::new(
                        XfstCommand::Redirected {
                            command: Box::new(cmd),
                            redirect,
                        },
                        self.merge(start),
                    ));
                }
                _ => {}
            }
        }
        Ok(cmd)
    }

    fn parse_command_inner(&mut self, start: usize) -> Result<Spanned<XfstCommand>, Diagnostic> {
        let kind = match self.peek().cloned() {
            Some(Token::Command(k)) => {
                self.bump();
                k
            }
            other => {
                return Err(Diagnostic::error(
                    self.peek_span(),
                    format!("expected command keyword, got {other:?}"),
                ));
            }
        };
        let cmd = match kind {
            CommandKind::ReadRegex => XfstCommand::Regex(self.expect_regex_body(start)?),
            CommandKind::DefineName => self.parse_define(start, false)?,
            CommandKind::DefineFunction => self.parse_define(start, true)?,
            CommandKind::DefineAlias => self.parse_alias()?,
            CommandKind::Undefine => self.parse_undefine()?,
            CommandKind::Unlist => XfstCommand::Unlist(self.expect_name("unlist target")?),
            CommandKind::List => self.parse_list()?,

            CommandKind::ApplyUp => XfstCommand::Apply(ApplyKind::Up, self.eat_apply_body()),
            CommandKind::ApplyDown => XfstCommand::Apply(ApplyKind::Down, self.eat_apply_body()),
            CommandKind::ApplyMed => XfstCommand::Apply(ApplyKind::Med, self.eat_apply_body()),
            CommandKind::ApplyUpSingle => {
                XfstCommand::Apply(ApplyKind::Up, Some(self.expect_name("apply input")?))
            }
            CommandKind::ApplyDownSingle => {
                XfstCommand::Apply(ApplyKind::Down, Some(self.expect_name("apply input")?))
            }
            CommandKind::LookupOptimize => XfstCommand::LookupOptimize,
            CommandKind::RemoveOptimization => XfstCommand::RemoveOptimization,

            CommandKind::Clear => XfstCommand::Clear,
            CommandKind::Pop => XfstCommand::Pop,
            CommandKind::PushDefined => XfstCommand::Push(self.expect_optional_name()),
            CommandKind::Turn => XfstCommand::Turn,
            CommandKind::Rotate => XfstCommand::Rotate,
            CommandKind::Loads => XfstCommand::LoadStack(self.expect_optional_name()),
            CommandKind::Loadd => XfstCommand::LoadDefinitions(self.expect_optional_name()),

            // Network unary
            CommandKind::Invert => XfstCommand::Network(NetworkOp::Invert),
            CommandKind::Reverse => XfstCommand::Network(NetworkOp::Reverse),
            CommandKind::Determinize => XfstCommand::Network(NetworkOp::Determinize),
            CommandKind::Minimize => XfstCommand::Network(NetworkOp::Minimize),
            CommandKind::EpsilonRemove => XfstCommand::Network(NetworkOp::EpsilonRemove),
            CommandKind::PruneNet => XfstCommand::Network(NetworkOp::PruneNet),
            CommandKind::Negate => XfstCommand::Network(NetworkOp::Negate),
            CommandKind::OnePlus => XfstCommand::Network(NetworkOp::OnePlus),
            CommandKind::ZeroPlus => XfstCommand::Network(NetworkOp::ZeroPlus),
            CommandKind::Sort => XfstCommand::Network(NetworkOp::Sort),
            CommandKind::Shuffle => XfstCommand::Network(NetworkOp::Shuffle),
            CommandKind::Substring => XfstCommand::Network(NetworkOp::Substring),
            CommandKind::Cleanup => XfstCommand::Network(NetworkOp::Cleanup),
            CommandKind::Complete => XfstCommand::Network(NetworkOp::Complete),
            CommandKind::LowerSide => XfstCommand::Network(NetworkOp::LowerSide),
            CommandKind::UpperSide => XfstCommand::Network(NetworkOp::UpperSide),
            CommandKind::Sigma => XfstCommand::Network(NetworkOp::Sigma),
            CommandKind::LabelNet => XfstCommand::Network(NetworkOp::LabelNet),
            CommandKind::Inspect => XfstCommand::Network(NetworkOp::Inspect),
            CommandKind::TwosidedFlags => XfstCommand::Network(NetworkOp::TwosidedFlags),
            CommandKind::EliminateAll => XfstCommand::Network(NetworkOp::EliminateAll),
            CommandKind::CollectEpsilonLoops => {
                XfstCommand::Network(NetworkOp::CollectEpsilonLoops)
            }
            CommandKind::CompactSigma => XfstCommand::Network(NetworkOp::CompactSigma),
            CommandKind::View => XfstCommand::Network(NetworkOp::View),
            CommandKind::ExtractAmbiguous => XfstCommand::Network(NetworkOp::ExtractAmbiguous),
            CommandKind::ExtractUnambiguous => XfstCommand::Network(NetworkOp::ExtractUnambiguous),
            CommandKind::Ambiguous => XfstCommand::Network(NetworkOp::Ambiguous),
            CommandKind::CompileReplaceLower => {
                XfstCommand::Network(NetworkOp::CompileReplaceLower)
            }
            CommandKind::CompileReplaceUpper => {
                XfstCommand::Network(NetworkOp::CompileReplaceUpper)
            }
            CommandKind::EliminateFlag => {
                XfstCommand::Network(NetworkOp::EliminateFlag(self.expect_name("flag name")?))
            }
            CommandKind::Name => XfstCommand::Network(NetworkOp::Name(self.expect_optional_name())),

            // Network binary
            CommandKind::Compose => XfstCommand::Network(NetworkOp::Compose),
            CommandKind::Concatenate => XfstCommand::Network(NetworkOp::Concatenate),
            CommandKind::Intersect => XfstCommand::Network(NetworkOp::Intersect),
            CommandKind::Union => XfstCommand::Network(NetworkOp::Union),
            CommandKind::Minus => XfstCommand::Network(NetworkOp::Minus),
            CommandKind::Crossproduct => XfstCommand::Network(NetworkOp::Crossproduct),
            CommandKind::XfstIgnore => XfstCommand::Network(NetworkOp::Ignore),

            // Print
            CommandKind::Print => XfstCommand::Print(PrintCmd::Net),
            CommandKind::PrintStack => XfstCommand::Print(PrintCmd::Stack),
            CommandKind::PrintSigma => XfstCommand::Print(PrintCmd::Sigma),
            CommandKind::PrintSigmaCount => XfstCommand::Print(PrintCmd::SigmaCount),
            CommandKind::PrintSigmaWordCount => XfstCommand::Print(PrintCmd::SigmaWordCount),
            CommandKind::PrintSize => XfstCommand::Print(PrintCmd::Size),
            CommandKind::PrintLongestString => XfstCommand::Print(PrintCmd::LongestString),
            CommandKind::PrintLongestStringSize => XfstCommand::Print(PrintCmd::LongestStringSize),
            CommandKind::PrintShortestString => XfstCommand::Print(PrintCmd::ShortestString),
            CommandKind::PrintShortestStringSize => {
                XfstCommand::Print(PrintCmd::ShortestStringSize)
            }
            CommandKind::PrintFlags => XfstCommand::Print(PrintCmd::Flags),
            CommandKind::PrintLabels => {
                let arg = if matches!(self.peek(), Some(Token::Name(_))) {
                    if let Some((Token::Name(s), _)) = self.bump() {
                        Some(s)
                    } else {
                        None
                    }
                } else {
                    None
                };
                XfstCommand::Print(PrintCmd::Labels(arg))
            }
            CommandKind::PrintLabelCount => XfstCommand::Print(PrintCmd::LabelCount),
            CommandKind::PrintLabelmaps => XfstCommand::Print(PrintCmd::LabelMaps),
            CommandKind::PrintName => XfstCommand::Print(PrintCmd::Name),
            CommandKind::PrintAliases => XfstCommand::Print(PrintCmd::Aliases),
            CommandKind::PrintArccount => XfstCommand::Print(PrintCmd::Arccount),
            CommandKind::PrintDefined => XfstCommand::Print(PrintCmd::Defined),
            CommandKind::PrintDir => XfstCommand::Print(PrintCmd::Dir),
            CommandKind::PrintFileInfo => XfstCommand::Print(PrintCmd::FileInfo),
            CommandKind::PrintList => XfstCommand::Print(PrintCmd::List),
            CommandKind::PrintLists => XfstCommand::Print(PrintCmd::Lists),
            CommandKind::PrintWords => XfstCommand::Print(PrintCmd::Words(self.eat_optional_u32())),
            CommandKind::PrintLowerWords => {
                XfstCommand::Print(PrintCmd::LowerWords(self.eat_optional_u32()))
            }
            CommandKind::PrintUpperWords => {
                XfstCommand::Print(PrintCmd::UpperWords(self.eat_optional_u32()))
            }
            CommandKind::PrintRandomWords => {
                XfstCommand::Print(PrintCmd::RandomWords(self.eat_optional_u32()))
            }
            CommandKind::PrintRandomLower => {
                XfstCommand::Print(PrintCmd::RandomLower(self.eat_optional_u32()))
            }
            CommandKind::PrintRandomUpper => {
                XfstCommand::Print(PrintCmd::RandomUpper(self.eat_optional_u32()))
            }
            CommandKind::PrintProps => XfstCommand::Print(PrintCmd::Props),

            // Save
            CommandKind::SaveStack => XfstCommand::Save(SaveCmd::Stack(self.expect_name("path")?)),
            CommandKind::SaveProlog => {
                XfstCommand::Save(SaveCmd::Prolog(self.expect_name("path")?))
            }
            CommandKind::SaveSpaced => {
                XfstCommand::Save(SaveCmd::Spaced(self.expect_name("path")?))
            }
            CommandKind::SaveText => XfstCommand::Save(SaveCmd::Text(self.expect_name("path")?)),
            CommandKind::SaveDot => XfstCommand::Save(SaveCmd::Dot(self.expect_name("path")?)),
            CommandKind::SaveDefinition => {
                XfstCommand::Save(SaveCmd::Definition(self.expect_name("path")?))
            }
            CommandKind::SaveDefinitions => {
                XfstCommand::Save(SaveCmd::Definitions(self.expect_name("path")?))
            }
            CommandKind::WriteAtt => XfstCommand::Save(SaveCmd::Att(self.expect_optional_name())),

            // Read
            CommandKind::ReadText => XfstCommand::Read(ReadCmd::Text(self.eat_heredoc_or_path())),
            CommandKind::ReadSpaced => {
                XfstCommand::Read(ReadCmd::Spaced(self.eat_heredoc_or_path()))
            }
            CommandKind::ReadProlog => {
                XfstCommand::Read(ReadCmd::Prolog(self.expect_name("path")?))
            }
            CommandKind::ReadProps => XfstCommand::Read(ReadCmd::Props(self.expect_name("path")?)),
            CommandKind::ReadLexc => XfstCommand::Read(ReadCmd::Lexc(self.expect_name("path")?)),
            CommandKind::ReadAtt => XfstCommand::Read(ReadCmd::Att(self.expect_name("path")?)),

            // Test
            CommandKind::TestEq => XfstCommand::Test(TestKind::Eq),
            CommandKind::TestFunct => XfstCommand::Test(TestKind::Funct),
            CommandKind::TestId => XfstCommand::Test(TestKind::Id),
            CommandKind::TestNull => XfstCommand::Test(TestKind::Null),
            CommandKind::TestNonnull => XfstCommand::Test(TestKind::Nonnull),
            CommandKind::TestOverlap => XfstCommand::Test(TestKind::Overlap),
            CommandKind::TestSublanguage => XfstCommand::Test(TestKind::Sublanguage),
            CommandKind::TestUnambiguous => XfstCommand::Test(TestKind::Unambiguous),
            CommandKind::TestInfinitelyAmbiguous => {
                XfstCommand::Test(TestKind::InfinitelyAmbiguous)
            }
            CommandKind::TestLowerBounded => XfstCommand::Test(TestKind::LowerBounded),
            CommandKind::TestLowerUni => XfstCommand::Test(TestKind::LowerUni),
            CommandKind::TestUpperBounded => XfstCommand::Test(TestKind::UpperBounded),
            CommandKind::TestUpperUni => XfstCommand::Test(TestKind::UpperUni),

            // System / shell
            CommandKind::Echo => XfstCommand::Echo(self.expect_optional_name()),
            CommandKind::Quit => XfstCommand::Quit,
            CommandKind::System => XfstCommand::System(self.expect_optional_name()),
            CommandKind::Source => XfstCommand::Source(self.expect_name("source path")?),
            CommandKind::Apropos => {
                let p = self.expect_optional_name();
                XfstCommand::Apropos(if p.is_empty() { None } else { Some(p) })
            }
            CommandKind::Describe => XfstCommand::Describe(self.expect_optional_name()),
            CommandKind::Hfst => XfstCommand::Hfst(self.expect_optional_name()),
            CommandKind::For => XfstCommand::For,
            CommandKind::Assert => {
                let inner = self.parse_command()?;
                XfstCommand::Assert(Box::new(inner))
            }

            // Variables / show
            CommandKind::Set => self.parse_set()?,
            CommandKind::Show => XfstCommand::Show(Some(self.expect_name("show target")?)),
            CommandKind::ShowAll => XfstCommand::Show(None),

            // Substitute
            CommandKind::SubstituteSymbol => self.parse_substitute(SubstKind::Symbol)?,
            CommandKind::SubstituteLabel => self.parse_substitute(SubstKind::Label)?,
            CommandKind::SubstituteNamed => self.parse_substitute_named()?,

            // Properties
            CommandKind::AddProps => XfstCommand::AddProps(self.expect_optional_name()),
            CommandKind::EditProps => XfstCommand::EditProps,
        };
        Ok(Spanned::new(cmd, self.merge(start)))
    }

    // ───────────────────────── helpers ─────────────────────────

    fn expect_regex_body(&mut self, _start: usize) -> Result<SpannedXre, Diagnostic> {
        let (body, span) = match self.bump() {
            Some((Token::RegexBody(b), s)) => (b, s),
            other => {
                return Err(Diagnostic::error(
                    self.peek_span(),
                    format!("expected regex body, got {other:?}"),
                ));
            }
        };
        // `define NAME ;` and `define NAME` (EOL) are declarations with
        // no body — represent the missing body as Epsilon rather than
        // failing the xre parse.
        if body.trim().is_empty() {
            return Ok(Spanned::new(nfst_xre::XreExpr::Epsilon, span));
        }
        match nfst_xre::parse(&body) {
            Ok(parsed) => Ok(Spanned::new(parsed.value, span)),
            Err(e) => Err(Diagnostic::error(span, format!("xre body: {e:?}"))),
        }
    }

    fn parse_define(
        &mut self,
        _start: usize,
        is_function: bool,
    ) -> Result<XfstCommand, Diagnostic> {
        let name = self.expect_name("definition name")?;
        let params = if is_function {
            self.expect_prototype_params()?
        } else {
            Vec::new()
        };
        let body = self.expect_regex_body(0)?;
        if is_function {
            Ok(XfstCommand::DefineFunction { name, params, body })
        } else {
            Ok(XfstCommand::Define { name, body })
        }
    }

    fn expect_prototype_params(&mut self) -> Result<Vec<String>, Diagnostic> {
        match self.bump() {
            Some((Token::Prototype(s), _)) => {
                let inner = s.trim_start_matches('(').trim_end_matches(')');
                let params = inner
                    .split(',')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect();
                Ok(params)
            }
            other => Err(Diagnostic::error(
                self.peek_span(),
                format!("expected `(args)` prototype, got {other:?}"),
            )),
        }
    }

    fn parse_alias(&mut self) -> Result<XfstCommand, Diagnostic> {
        let name = self.expect_name("alias name")?;
        // Body: collect remaining tokens until `;`, including command
        // keywords (alias bodies are arbitrary command sequences).
        let mut parts = Vec::new();
        while !matches!(self.peek(), Some(Token::Semicolon) | None) {
            match self.bump() {
                Some((Token::Name(s), _)) => parts.push(s),
                Some((Token::Command(k), _)) => parts.push(format!("{k:?}").to_lowercase()),
                Some((Token::Colon, _)) => parts.push(":".into()),
                Some((Token::Comma, _)) => parts.push(",".into()),
                Some((_, _)) => continue,
                None => break,
            }
        }
        Ok(XfstCommand::DefineAlias {
            name,
            body: parts.join(" "),
        })
    }

    fn parse_undefine(&mut self) -> Result<XfstCommand, Diagnostic> {
        let mut names = Vec::new();
        while let Some(Token::Name(_)) = self.peek() {
            if let Some((Token::Name(s), _)) = self.bump() {
                names.push(s);
            }
        }
        Ok(XfstCommand::Undefine(names))
    }

    fn parse_list(&mut self) -> Result<XfstCommand, Diagnostic> {
        let name = self.expect_name("list name")?;
        let mut members = Vec::new();
        while !matches!(self.peek(), Some(Token::Semicolon) | None) {
            match self.bump() {
                Some((Token::Name(s), _)) => members.push(s),
                Some((Token::Range(a, b), _)) => {
                    members.push(format!("{a}-{b}"));
                }
                Some((other, span)) => {
                    return Err(Diagnostic::error(
                        span,
                        format!("unexpected token in list body: {other:?}"),
                    ));
                }
                None => break,
            }
        }
        Ok(XfstCommand::DefineList { name, members })
    }

    fn parse_set(&mut self) -> Result<XfstCommand, Diagnostic> {
        let var = self.expect_name("variable name")?;
        let value = self.expect_optional_name();
        Ok(XfstCommand::Set { var, value })
    }

    fn parse_substitute(&mut self, kind: SubstKind) -> Result<XfstCommand, Diagnostic> {
        // Upstream shapes (NAMETOKEN_LIST permits multiple targets):
        //   substitute symbol N1 N2 ... for LABEL [END]
        //   substitute label  L1 L2 ... for LABEL [END]
        // where LABEL = `name:name` (or just a bare name).
        let mut from = Vec::new();
        loop {
            // Stop when we hit `for`, `;`, or end-of-stream.
            if matches!(
                self.peek(),
                Some(Token::Command(CommandKind::For))
                    | Some(Token::Semicolon)
                    | Some(Token::EndSub)
                    | None
            ) {
                break;
            }
            let lbl = match kind {
                SubstKind::Symbol => self.expect_name("substitute target")?,
                SubstKind::Label => self.read_label("substitute label")?,
            };
            from.push(lbl);
        }
        if from.is_empty() {
            return Err(Diagnostic::error(
                self.peek_span(),
                "substitute requires at least one target before `for`",
            ));
        }
        self.eat_optional_for();
        let to = self.read_label("substitute replacement")?;
        let scope = if matches!(self.peek(), Some(Token::Name(_))) {
            Some(self.expect_name("substitute scope")?)
        } else {
            None
        };
        self.eat_optional_end_sub();
        let cmd = match kind {
            SubstKind::Symbol => SubstituteCmd::Symbol { from, to, scope },
            SubstKind::Label => SubstituteCmd::Label { from, to, scope },
        };
        Ok(XfstCommand::Substitute(cmd))
    }

    fn parse_substitute_named(&mut self) -> Result<XfstCommand, Diagnostic> {
        // `substitute defined NAME for LABEL [END]`
        let def = self.expect_name("substitute defined target")?;
        self.eat_optional_for();
        let label = self.read_label("substitute defined label")?;
        self.eat_optional_end_sub();
        Ok(XfstCommand::Substitute(SubstituteCmd::Named { def, label }))
    }

    /// Read a label of the form `NAME[:NAME]`. The colon, when present,
    /// is preserved in the returned string. Either side may be empty
    /// (`NAME:` or `:NAME`) — upstream allows both.
    fn read_label(&mut self, what: &str) -> Result<String, Diagnostic> {
        // Leading `:NAME` form.
        if matches!(self.peek(), Some(Token::Colon)) {
            self.bump();
            let lower = self.expect_name(what)?;
            return Ok(format!(":{lower}"));
        }
        let upper = self.expect_name(what)?;
        if matches!(self.peek(), Some(Token::Colon)) {
            self.bump();
            // Allow trailing `NAME:` form (lower is empty/wildcard).
            if matches!(self.peek(), Some(Token::Name(_)))
                && let Some((Token::Name(lower), _)) = self.bump()
            {
                return Ok(format!("{upper}:{lower}"));
            }
            return Ok(format!("{upper}:"));
        }
        Ok(upper)
    }

    fn eat_optional_for(&mut self) {
        if matches!(self.peek(), Some(Token::Command(CommandKind::For))) {
            self.bump();
        }
    }

    fn eat_optional_end_sub(&mut self) {
        if matches!(self.peek(), Some(Token::EndSub)) {
            self.bump();
        }
    }

    fn expect_name(&mut self, label: &str) -> Result<String, Diagnostic> {
        match self.bump() {
            Some((Token::Name(s), _)) => Ok(s),
            other => Err(Diagnostic::error(
                self.peek_span(),
                format!("expected {label}, got {other:?}"),
            )),
        }
    }

    fn expect_optional_name(&mut self) -> String {
        if let Some(Token::Name(_)) = self.peek()
            && let Some((Token::Name(s), _)) = self.bump()
        {
            return s;
        }
        String::new()
    }

    fn eat_apply_body(&mut self) -> Option<String> {
        if let Some(Token::ApplyBody(_)) = self.peek()
            && let Some((Token::ApplyBody(s), _)) = self.bump()
        {
            return Some(s);
        }
        None
    }

    fn eat_heredoc_or_path(&mut self) -> String {
        match self.peek() {
            Some(Token::HeredocBody(_)) => {
                if let Some((Token::HeredocBody(s), _)) = self.bump() {
                    return s;
                }
            }
            Some(Token::Name(_)) => {
                if let Some((Token::Name(s), _)) = self.bump() {
                    return s;
                }
            }
            _ => {}
        }
        String::new()
    }

    fn eat_optional_u32(&mut self) -> Option<u32> {
        if let Some(Token::Name(s)) = self.peek()
            && let Ok(n) = s.parse::<u32>()
        {
            self.bump();
            return Some(n);
        }
        None
    }
}

enum SubstKind {
    Symbol,
    Label,
}
