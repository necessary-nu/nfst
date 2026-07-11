//! Hand-rolled lexer for xfst.
//!
//! State-aware: multi-word command keywords like `compose net` collapse
//! to a single token, and the lexer switches modes after `regex` /
//! `define NAME …` (capture regex body until `;`) and `apply up/down/med`
//! / `read text` (capture heredoc body until `<ctrl-d>` marker).

use crate::token::{CommandKind, Token};
use nfst_syntax::Span;

#[derive(Clone, Debug, PartialEq)]
pub struct LexError {
    pub span: Span,
    pub message: String,
}

pub fn tokenize(src: &str) -> Result<Vec<(Token, Span)>, Vec<LexError>> {
    let mut l = Lexer::new(src);
    l.run();
    if l.errors.is_empty() {
        Ok(l.tokens)
    } else {
        Err(l.errors)
    }
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    tokens: Vec<(Token, Span)>,
    errors: Vec<LexError>,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src: src.as_bytes(),
            pos: 0,
            tokens: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn run(&mut self) {
        while self.pos < self.src.len() {
            self.skip_trivia();
            if self.pos >= self.src.len() {
                break;
            }
            let start = self.pos;
            let b = self.src[self.pos];
            match b {
                b';' => {
                    self.pos += 1;
                    self.push(Token::Semicolon, start);
                }
                b':' => {
                    self.pos += 1;
                    self.push(Token::Colon, start);
                }
                b',' => {
                    self.pos += 1;
                    self.push(Token::Comma, start);
                }
                b'[' => {
                    self.pos += 1;
                    self.push(Token::LeftBracket, start);
                }
                b']' => {
                    self.pos += 1;
                    self.push(Token::RightBracket, start);
                }
                b')' => {
                    self.pos += 1;
                    self.push(Token::RightParen, start);
                }
                b'(' => {
                    // `(a, b, c)` may be a function prototype; otherwise plain `(`.
                    if let Some(proto) = self.try_prototype() {
                        self.push(Token::Prototype(proto), start);
                    } else {
                        self.pos += 1;
                        self.push(Token::LeftParen, start);
                    }
                }
                b'>' => {
                    self.pos += 1;
                    if self.pos < self.src.len() && self.src[self.pos] == b'>' {
                        self.pos += 1;
                        self.skip_horizontal_ws();
                        let path = self.read_nametoken();
                        self.push(Token::RedirectAppend(path), start);
                    } else {
                        self.skip_horizontal_ws();
                        let path = self.read_nametoken();
                        self.push(Token::RedirectOut(path), start);
                    }
                }
                b'<' => {
                    self.pos += 1;
                    self.skip_horizontal_ws();
                    let path = self.read_nametoken();
                    self.push(Token::RedirectIn(path), start);
                }
                0x04 => {
                    self.pos += 1;
                    self.push(Token::CtrlD, start);
                }
                _ => self.token_or_keyword(start),
            }
        }
    }

    /// Skip whitespace and `!` / `#` line comments.
    fn skip_trivia(&mut self) {
        loop {
            while self.pos < self.src.len() {
                match self.src[self.pos] {
                    b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                    _ => break,
                }
            }
            if self.pos < self.src.len()
                && (self.src[self.pos] == b'!' || self.src[self.pos] == b'#')
            {
                while self.pos < self.src.len() && self.src[self.pos] != b'\n' {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn skip_horizontal_ws(&mut self) {
        while self.pos < self.src.len()
            && (self.src[self.pos] == b' ' || self.src[self.pos] == b'\t')
        {
            self.pos += 1;
        }
    }

    fn push(&mut self, t: Token, start: usize) {
        self.tokens.push((t, Span::anonymous(start..self.pos)));
    }

    /// Try matching a command keyword at `self.pos`. If a match is
    /// found, dispatch into mode-specific handling (regex body, heredoc,
    /// inline text) as needed. Otherwise fall back to NAMETOKEN.
    fn token_or_keyword(&mut self, start: usize) {
        if let Some((kind, len)) = match_keyword(&self.src[self.pos..]) {
            self.pos += len;
            // Special handling per command kind.
            match kind {
                // regex bodies: capture until top-level `;`
                CommandKind::ReadRegex => {
                    self.push(Token::Command(kind), start);
                    self.read_regex_body();
                }
                CommandKind::DefineName | CommandKind::DefineFunction => {
                    // Read the name first, then peek for `(args)` — if
                    // present, upgrade to DefineFunction.
                    self.skip_horizontal_ws();
                    let name_start = self.pos;
                    let name = self.read_nametoken();
                    let name_end = self.pos;
                    self.skip_horizontal_ws();
                    let mut effective = kind;
                    let mut proto: Option<(String, usize, usize)> = None;
                    if self.pos < self.src.len() && self.src[self.pos] == b'(' {
                        let proto_start = self.pos;
                        if let Some(p) = self.try_prototype() {
                            proto = Some((p, proto_start, self.pos));
                            effective = CommandKind::DefineFunction;
                        }
                    }
                    self.push(Token::Command(effective), start);
                    if !name.is_empty() {
                        self.tokens
                            .push((Token::Name(name), Span::anonymous(name_start..name_end)));
                    }
                    if let Some((p, ps, pe)) = proto {
                        self.tokens
                            .push((Token::Prototype(p), Span::anonymous(ps..pe)));
                    }
                    self.read_regex_body();
                }
                CommandKind::ApplyUp | CommandKind::ApplyDown | CommandKind::ApplyMed => {
                    // Heredoc form when nothing else follows on the same
                    // line; otherwise the rest of the line is the inline
                    // payload. We need to detect which BEFORE emitting
                    // the command, so we can pick ApplyUpSingle vs.
                    // ApplyUp accordingly and keep token order
                    // command-first.
                    self.dispatch_apply(kind, start);
                }
                CommandKind::ReadText | CommandKind::ReadSpaced => {
                    self.push(Token::Command(kind), start);
                    self.read_heredoc_body();
                }
                CommandKind::Echo
                | CommandKind::System
                | CommandKind::Apropos
                | CommandKind::Describe
                | CommandKind::Hfst => {
                    // These take "rest of line" as a single text payload.
                    self.push(Token::Command(kind), start);
                    self.skip_horizontal_ws();
                    let line = self.read_to_end_of_line();
                    if !line.is_empty() {
                        let lstart = self.pos - line.len();
                        self.tokens
                            .push((Token::Name(line), Span::anonymous(lstart..self.pos)));
                    }
                }
                _ => {
                    self.push(Token::Command(kind), start);
                }
            }
            return;
        }

        // No keyword matched. Read a free-form NAMETOKEN.
        let name = self.read_nametoken();
        if name.is_empty() {
            // Couldn't progress — record an error and skip one byte.
            let bad = self.src[self.pos] as char;
            let span = Span::anonymous(self.pos..self.pos + 1);
            self.errors.push(LexError {
                span,
                message: format!("unexpected character {bad:?}"),
            });
            self.pos += 1;
            return;
        }
        // Range: `a-b` (only meaningful in `list` contexts; we recognise
        // it eagerly when shaped right).
        if self.pos < self.src.len()
            && self.src[self.pos] == b'-'
            && self.pos + 1 < self.src.len()
            && is_name_start(self.src[self.pos + 1])
        {
            self.pos += 1;
            let upper = self.read_nametoken();
            self.push(Token::Range(name, upper), start);
            return;
        }
        // `END` / `END;` — substitution terminator.
        if name == "END" {
            self.push(Token::EndSub, start);
            return;
        }
        self.push(Token::Name(name), start);
    }

    /// Attempt to match a parenthesised prototype like `(a, b, c)`. Only
    /// succeeds if every interior character is alnum, `_`, space, or
    /// `,` — matches upstream's `[a-zA-Z_0-9 ,]*`.
    fn try_prototype(&mut self) -> Option<String> {
        if self.pos >= self.src.len() || self.src[self.pos] != b'(' {
            return None;
        }
        let start = self.pos;
        let mut p = start + 1;
        while p < self.src.len() {
            let c = self.src[p];
            if c == b')' {
                let s = std::str::from_utf8(&self.src[start..=p]).ok()?.to_string();
                self.pos = p + 1;
                return Some(s);
            }
            if !(c.is_ascii_alphanumeric() || c == b'_' || c == b' ' || c == b',' || c == b'\t') {
                return None;
            }
            p += 1;
        }
        None
    }

    /// Read a NAMETOKEN, decoding `%X` escapes (the `%` is consumed; the
    /// next byte is taken literally).
    fn read_nametoken(&mut self) -> String {
        let mut out = String::new();
        while self.pos < self.src.len() {
            let b = self.src[self.pos];
            if b == b'%' && self.pos + 1 < self.src.len() {
                self.pos += 1;
                let nb = self.src[self.pos];
                if nb >= 0x80 {
                    self.push_utf8_codepoint(&mut out);
                } else {
                    out.push(nb as char);
                    self.pos += 1;
                }
                continue;
            }
            if b >= 0x80 {
                self.push_utf8_codepoint(&mut out);
                continue;
            }
            if is_name_continue(b) {
                out.push(b as char);
                self.pos += 1;
                continue;
            }
            break;
        }
        out
    }

    fn push_utf8_codepoint(&mut self, out: &mut String) {
        let start = self.pos;
        let lead = self.src[start];
        let len = if lead < 0xc0 {
            // ASCII (<0x80) or a stray continuation byte (0x80..0xc0) — 1 byte either way.
            1
        } else if lead < 0xe0 {
            2
        } else if lead < 0xf0 {
            3
        } else {
            4
        };
        let end = (start + len).min(self.src.len());
        if let Ok(s) = std::str::from_utf8(&self.src[start..end]) {
            out.push_str(s);
            self.pos = end;
        } else {
            // Skip one byte to avoid infinite loop.
            self.pos = start + 1;
        }
    }

    /// Read until end of current line (no newline included). Used for
    /// `echo`, `system`, `apropos`, `help`, `hfst` payloads.
    fn read_to_end_of_line(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.src.len() {
            let b = self.src[self.pos];
            if b == b'\n' || b == b'\r' {
                break;
            }
            self.pos += 1;
        }
        // Trim trailing whitespace.
        let end = self.src[start..self.pos]
            .iter()
            .rposition(|c| !matches!(c, b' ' | b'\t'))
            .map(|i| start + i + 1)
            .unwrap_or(start);
        String::from_utf8_lossy(&self.src[start..end]).into_owned()
    }

    /// Capture a regex body up to (and not including) the next top-level
    /// `;`. The semicolon stays in the stream for the parser to consume.
    ///
    /// If, after horizontal whitespace, the next character is a newline
    /// or `;`, emit an empty body. This matches upstream's behavior for
    /// `define NAME` declarations with no body.
    fn read_regex_body(&mut self) {
        self.skip_horizontal_ws();
        // Empty body — declaration form like `define NAME` (EOL) or
        // `define NAME ;`.
        if self.pos >= self.src.len()
            || self.src[self.pos] == b'\n'
            || self.src[self.pos] == b'\r'
            || self.src[self.pos] == b';'
        {
            let span = Span::anonymous(self.pos..self.pos);
            self.tokens.push((Token::RegexBody(String::new()), span));
            return;
        }
        let start = self.pos;
        // Track bracket depth so a `;` inside `[…]` or `(…)` doesn't end
        // the regex.
        let mut depth = 0i32;
        let mut in_quote: Option<u8> = None;
        while self.pos < self.src.len() {
            let b = self.src[self.pos];
            if let Some(q) = in_quote {
                if b == b'%' && self.pos + 1 < self.src.len() {
                    self.pos += 2;
                    continue;
                }
                if b == q {
                    in_quote = None;
                }
                self.pos += 1;
                continue;
            }
            if b == b'%' && self.pos + 1 < self.src.len() {
                // Escaped char counts as part of the body.
                self.pos += 2;
                continue;
            }
            if b == b'"' {
                in_quote = Some(b'"');
                self.pos += 1;
                continue;
            }
            if b == b'[' || b == b'(' || b == b'{' {
                depth += 1;
                self.pos += 1;
                continue;
            }
            if b == b']' || b == b')' || b == b'}' {
                depth -= 1;
                self.pos += 1;
                continue;
            }
            if b == b';' && depth <= 0 {
                break;
            }
            self.pos += 1;
        }
        // Trim only leading whitespace — trailing space may be part of
        // a `% ` (escaped-space) sequence at the end of the body.
        let body = String::from_utf8_lossy(&self.src[start..self.pos])
            .trim_start()
            .to_string();
        self.tokens
            .push((Token::RegexBody(body), Span::anonymous(start..self.pos)));
    }

    /// Detect whether `apply up/down/med` is followed by inline text or
    /// is in heredoc form, emit the right command kind, then emit the
    /// body. Token order is always (command, body).
    fn dispatch_apply(&mut self, kind: CommandKind, start: usize) {
        let mut p = self.pos;
        while p < self.src.len() && (self.src[p] == b' ' || self.src[p] == b'\t') {
            p += 1;
        }
        let inline = p < self.src.len() && self.src[p] != b'\n' && self.src[p] != b'\r';
        if inline {
            // Promote to single-line form.
            let single = match kind {
                CommandKind::ApplyUp => CommandKind::ApplyUpSingle,
                CommandKind::ApplyDown => CommandKind::ApplyDownSingle,
                CommandKind::ApplyMed => CommandKind::ApplyMed,
                other => other,
            };
            self.push(Token::Command(single), start);
            self.pos = p;
            let line = self.read_to_end_of_line();
            let lstart = self.pos - line.len();
            self.tokens
                .push((Token::Name(line), Span::anonymous(lstart..self.pos)));
            return;
        }
        self.push(Token::Command(kind), start);
        if p < self.src.len() && (self.src[p] == b'\n' || self.src[p] == b'\r') {
            self.pos = p + 1;
            self.read_apply_body();
        }
    }

    fn read_apply_body(&mut self) {
        let start = self.pos;
        let marker = b"<ctrl-d>";
        while self.pos < self.src.len() {
            // Look for literal "<ctrl-d>".
            if self.src[self.pos..].starts_with(marker) {
                let body = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
                let span = Span::anonymous(start..self.pos);
                self.tokens.push((Token::ApplyBody(body), span));
                self.pos += marker.len();
                return;
            }
            if self.src[self.pos] == 0x04 {
                let body = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
                let span = Span::anonymous(start..self.pos);
                self.tokens.push((Token::ApplyBody(body), span));
                self.pos += 1;
                return;
            }
            self.pos += 1;
        }
        // No terminator — body runs to end of input.
        let body = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
        let span = Span::anonymous(start..self.pos);
        self.tokens.push((Token::ApplyBody(body), span));
    }

    fn read_heredoc_body(&mut self) {
        // Same shape as apply: consume until <ctrl-d> or 0x04 or EOF.
        // Skip whitespace and one trailing newline if present.
        self.skip_horizontal_ws();
        if self.pos < self.src.len() && (self.src[self.pos] == b'\n' || self.src[self.pos] == b'\r')
        {
            self.pos += 1;
        }
        let start = self.pos;
        let marker = b"<ctrl-d>";
        while self.pos < self.src.len() {
            if self.src[self.pos..].starts_with(marker) {
                let body = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
                let span = Span::anonymous(start..self.pos);
                self.tokens.push((Token::HeredocBody(body), span));
                self.pos += marker.len();
                return;
            }
            if self.src[self.pos] == 0x04 {
                let body = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
                let span = Span::anonymous(start..self.pos);
                self.tokens.push((Token::HeredocBody(body), span));
                self.pos += 1;
                return;
            }
            self.pos += 1;
        }
        let body = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
        let span = Span::anonymous(start..self.pos);
        self.tokens.push((Token::HeredocBody(body), span));
    }
}

fn is_name_start(b: u8) -> bool {
    is_name_continue(b)
}

fn is_name_continue(b: u8) -> bool {
    if b >= 0x80 {
        return true;
    }
    if b <= 0x20 || b == 0x7f {
        return false;
    }
    !matches!(
        b,
        b' ' | b'\t'
            | b'\n'
            | b'\r'
            | b'<'
            | b'>'
            | b'('
            | b')'
            | b'['
            | b']'
            | b'!'
            | b';'
            | b':'
            | b'"'
            | b'#'
            | b','
    )
}

/// Try to match a command keyword at the start of `bytes`. Returns the
/// matched [`CommandKind`] and the byte length consumed.
fn match_keyword(bytes: &[u8]) -> Option<(CommandKind, usize)> {
    // Multi-word forms must come before their shorter prefixes.
    for (kw, kind) in KEYWORDS {
        let kb = kw.as_bytes();
        if bytes.starts_with(kb) {
            // Word boundary: next byte (if any) must NOT be a name-continue
            // character. This prevents `definelike` from matching `define`.
            let next = bytes.get(kb.len()).copied();
            let boundary = match next {
                None => true,
                Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => true,
                Some(b';') | Some(b':') | Some(b'(') | Some(b')') => true,
                Some(b'[') | Some(b']') | Some(b'>') | Some(b'<') => true,
                Some(b',') | Some(b'!') | Some(b'#') | Some(0x04) => true,
                Some(_) => false,
            };
            if boundary {
                return Some((*kind, kb.len()));
            }
        }
    }
    None
}

/// Static table of (keyword string, command kind), longest-first so
/// `compose net` matches before `compose`. Rust evaluates this once at
/// program start.
static KEYWORDS: &[(&str, CommandKind)] = {
    use CommandKind::*;
    &[
        // ── 3-word forms ──────────────────────────────
        ("test infinitely ambiguous", TestInfinitelyAmbiguous),
        ("test infinitely-ambiguous", TestInfinitelyAmbiguous),
        ("twosided flag-diacritics", TwosidedFlags),
        ("collect epsilon-loops", CollectEpsilonLoops),
        ("compile-replace lower", CompileReplaceLower),
        ("compile-replace upper", CompileReplaceUpper),
        ("print sigma-word-tally", PrintSigmaWordCount),
        ("print shortest-string-size", PrintShortestStringSize),
        ("print shortest-string-length", PrintShortestStringSize),
        ("print longest-string-size", PrintLongestStringSize),
        ("print random-words", PrintRandomWords),
        ("print random-lower", PrintRandomLower),
        ("print random-upper", PrintRandomUpper),
        ("print upper-words", PrintUpperWords),
        ("print lower-words", PrintLowerWords),
        ("print longest-string", PrintLongestString),
        ("print shortest-string", PrintShortestString),
        ("print sigma-tally", PrintSigmaCount),
        ("print arc-tally", PrintArccount),
        ("print label-tally", PrintLabelCount),
        ("print label-maps", PrintLabelmaps),
        ("print file-info", PrintFileInfo),
        ("print directory", PrintDir),
        ("print defined", PrintDefined),
        ("print aliases", PrintAliases),
        ("print labels", PrintLabels),
        ("print stack", PrintStack),
        ("print sigma", PrintSigma),
        ("print flags", PrintFlags),
        ("print lists", PrintLists),
        ("print words", PrintWords),
        ("print props", PrintProps),
        ("print list", PrintList),
        ("print name", PrintName),
        ("print size", PrintSize),
        ("print net", Print),
        ("write properties", PrintProps),
        ("write definitions", SaveDefinitions),
        ("write definition", SaveDefinition),
        ("write spaced-text", SaveSpaced),
        ("write prolog", SaveProlog),
        ("write text", SaveText),
        ("write att", WriteAtt),
        ("write dot", SaveDot),
        ("read properties", ReadProps),
        ("read spaced-text", ReadSpaced),
        ("read prolog", ReadProlog),
        ("read regex", ReadRegex),
        ("read text", ReadText),
        ("read lexc", ReadLexc),
        ("read att", ReadAtt),
        ("save defined", SaveDefinitions),
        ("save stack", SaveStack),
        ("load defined", Loadd),
        ("load stack", Loads),
        ("test equivalent", TestEq),
        ("test functional", TestFunct),
        ("test identity", TestId),
        ("test lower-bounded", TestLowerBounded),
        ("test lower-universal", TestLowerUni),
        ("test non-null", TestNonnull),
        ("test null", TestNull),
        ("test overlap", TestOverlap),
        ("test sublanguage", TestSublanguage),
        ("test upper-bounded", TestUpperBounded),
        ("test upper-universal", TestUpperUni),
        ("test unambiguous", TestUnambiguous),
        ("substitute defined", SubstituteNamed),
        ("substitute symbol", SubstituteSymbol),
        ("substitute label", SubstituteLabel),
        ("ambiguous upper", Ambiguous),
        ("eliminate flags", EliminateAll),
        ("eliminate flag", EliminateFlag),
        ("epsilon-remove net", EpsilonRemove),
        ("crossproduct net", Crossproduct),
        ("intersect net", Intersect),
        ("compose net", Compose),
        ("concatenate net", Concatenate),
        ("determinize net", Determinize),
        ("determinise net", Determinize),
        ("minimize net", Minimize),
        ("minus net", Minus),
        ("negate net", Negate),
        ("one-plus net", OnePlus),
        ("zero-plus net", ZeroPlus),
        ("invert net", Invert),
        ("reverse net", Reverse),
        ("ignore net", XfstIgnore),
        ("inspect net", Inspect),
        ("union net", Union),
        ("shuffle net", Shuffle),
        ("substring net", Substring),
        ("upper-side net", UpperSide),
        ("lower-side net", LowerSide),
        ("complete net", Complete),
        ("cleanup net", Cleanup),
        ("prune net", PruneNet),
        ("label net", LabelNet),
        ("name net", Name),
        ("view net", View),
        ("sigma net", Sigma),
        ("sort net", Sort),
        ("clear stack", Clear),
        ("pop stack", Pop),
        ("turn stack", Turn),
        ("rotate stack", Rotate),
        ("show variables", ShowAll),
        ("show variable", Show),
        ("extract ambiguous", ExtractAmbiguous),
        ("extract unambiguous", ExtractUnambiguous),
        ("compact sigma", CompactSigma),
        ("epsilon-loops", CollectEpsilonLoops),
        ("apply down", ApplyDown),
        ("apply med", ApplyMed),
        ("apply up", ApplyUp),
        ("push defined", PushDefined),
        ("add properties", AddProps),
        ("edit properties", EditProps),
        ("lookup-optimize", LookupOptimize),
        ("lookup-optimise", LookupOptimize),
        ("remove-optimization", RemoveOptimization),
        ("remove-optimisation", RemoveOptimization),
        ("longest-string-size", PrintLongestStringSize),
        ("longest-string", PrintLongestString),
        ("shortest-string-size", PrintShortestStringSize),
        ("shortest-string", PrintShortestString),
        ("random-words", PrintRandomWords),
        ("random-lower", PrintRandomLower),
        ("random-upper", PrintRandomUpper),
        ("upper-words", PrintUpperWords),
        ("lower-words", PrintLowerWords),
        ("upper-side", UpperSide),
        ("lower-side", LowerSide),
        ("upper-bounded", TestUpperBounded),
        ("upper-universal", TestUpperUni),
        ("lower-bounded", TestLowerBounded),
        ("lower-universal", TestLowerUni),
        ("infinitely-ambiguous", TestInfinitelyAmbiguous),
        ("com-rep lower", CompileReplaceLower),
        ("com-rep upper", CompileReplaceUpper),
        ("au revoir", Quit),
        ("auf wiedersehen", Quit),
        ("hyvästi", Quit),
        ("näkemiin", Quit),
        ("viszlát", Quit),
        // ── single-word forms ────────────────────────
        ("ambiguous", Ambiguous),
        ("apropos", Apropos),
        ("aliases", PrintAliases),
        ("assert", Assert),
        ("alias", DefineAlias),
        ("att", ReadAtt),
        ("arc-tally", PrintArccount),
        ("add", AddProps),
        ("bye", Quit),
        ("cleanup", Cleanup),
        ("clear", Clear),
        ("complete", Complete),
        ("compose", Compose),
        ("concatenate", Concatenate),
        ("conjunct", Intersect),
        ("crossproduct", Crossproduct),
        ("describe", Describe),
        ("determinize", Determinize),
        ("determinise", Determinise_alias()),
        ("define", DefineName),
        ("disjunct", Union),
        ("directory", PrintDir),
        ("dot", SaveDot),
        ("down", ApplyDown),
        ("echo", Echo),
        ("edit", EditProps),
        ("epsilon-remove", EpsilonRemove),
        ("equivalent", TestEq),
        ("exit", Quit),
        ("file-info", PrintFileInfo),
        ("flags", PrintFlags),
        ("for", For),
        ("functional", TestFunct),
        ("hfst", Hfst),
        ("has", Quit),
        ("help", Describe),
        ("identity", TestId),
        ("ignore", XfstIgnore),
        ("inspect", Inspect),
        ("intersect", Intersect),
        ("invert", Invert),
        ("labels", PrintLabels),
        ("label-maps", PrintLabelmaps),
        ("label-tally", PrintLabelCount),
        ("lexc", ReadLexc),
        ("list", List),
        ("loadd", Loadd),
        ("load", Loads),
        ("longest-string", PrintLongestString),
        ("med", ApplyMed),
        ("minimize", Minimize),
        ("minimise", Minimize),
        ("minus", Minus),
        ("name", Name),
        ("negate", Negate),
        ("one-plus", OnePlus),
        ("overlap", TestOverlap),
        ("pdefined", PrintDefined),
        ("plz", PrintLongestStringSize),
        ("pls", PrintLongestString),
        ("psz", PrintShortestStringSize),
        ("pss", PrintShortestString),
        ("pname", PrintName),
        ("pop", Pop),
        ("prune", PruneNet),
        ("push", PushDefined),
        ("quit", Quit),
        ("regex", ReadRegex),
        ("remove-optimization", RemoveOptimization),
        ("reverse", Reverse),
        ("rotate", Rotate),
        ("rprops", ReadProps),
        ("rpl", ReadProlog),
        ("rs", ReadSpaced),
        ("rt", ReadText),
        ("save", SaveStack),
        ("saved", SaveDefinitions),
        ("set", Set),
        ("show", Show),
        ("shuffle", Shuffle),
        ("sigma", PrintSigma),
        ("sigma-tally", PrintSigmaCount),
        ("sitally", PrintSigmaCount),
        ("size", PrintSize),
        ("sort", Sort),
        ("source", Source),
        ("ss", SaveStack),
        ("stack", PrintStack),
        ("stop", Quit),
        ("sublanguage", TestSublanguage),
        ("substring", Substring),
        ("subtract", Minus),
        ("system", System),
        ("te", TestEq),
        ("tf", TestFunct),
        ("ti", TestId),
        ("tia", TestInfinitelyAmbiguous),
        ("tlb", TestLowerBounded),
        ("tlu", TestLowerUni),
        ("tnn", TestNonnull),
        ("tnu", TestNull),
        ("to", TestOverlap),
        ("ts", TestSublanguage),
        ("tub", TestUpperBounded),
        ("tuu", TestUpperUni),
        ("tu", TestUnambiguous),
        ("tfd", TwosidedFlags),
        ("turn", Turn),
        ("undefine", Undefine),
        ("unlist", Unlist),
        ("union", Union),
        ("up", ApplyUp),
        ("view", View),
        ("wdef", SaveDefinition),
        ("wdefs", SaveDefinitions),
        ("wdot", SaveDot),
        ("words", PrintWords),
        ("wpl", SaveProlog),
        ("wspaced-text", SaveSpaced),
        ("wt", SaveText),
        ("zero-plus", ZeroPlus),
    ]
};

#[allow(non_snake_case)]
const fn Determinise_alias() -> CommandKind {
    CommandKind::Determinize
}
