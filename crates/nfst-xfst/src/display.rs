//! Pretty-printer for xfst scripts. Each command on its own line,
//! terminated with `;`. Embedded xre bodies are pretty-printed via
//! [`nfst_xre`].

use crate::ast::{
    ApplyKind, NetworkOp, PrintCmd, ReadCmd, Redirect, RedirectKind, SaveCmd, SubstituteCmd,
    TestKind, XfstCommand, XfstScript,
};
use nfst_syntax::Spanned;
use std::fmt::Write;

pub fn pretty_print(script: &Spanned<XfstScript>) -> String {
    let mut out = String::new();
    for c in &script.value.commands {
        write_command(&mut out, &c.value);
        out.push('\n');
    }
    out
}

fn write_command(out: &mut String, c: &XfstCommand) {
    match c {
        XfstCommand::Regex(body) => {
            out.push_str("regex ");
            out.push_str(&nfst_xre::pretty_print(body));
            out.push_str(" ;");
        }
        XfstCommand::Define { name, body } => {
            let _ = write!(out, "define {} ", escape_name(name));
            out.push_str(&nfst_xre::pretty_print(body));
            out.push_str(" ;");
        }
        XfstCommand::DefineFunction { name, params, body } => {
            let _ = write!(out, "define {}({}) ", escape_name(name), params.join(", "));
            out.push_str(&nfst_xre::pretty_print(body));
            out.push_str(" ;");
        }
        XfstCommand::DefineAlias { name, body } => {
            let _ = write!(out, "alias {} {} ;", escape_name(name), body);
        }
        XfstCommand::DefineList { name, members } => {
            let _ = write!(out, "list {}", escape_name(name));
            for m in members {
                out.push(' ');
                out.push_str(&escape_name(m));
            }
            out.push_str(" ;");
        }
        XfstCommand::Undefine(names) => {
            out.push_str("undefine");
            for n in names {
                out.push(' ');
                out.push_str(&escape_name(n));
            }
            out.push_str(" ;");
        }
        XfstCommand::Unlist(name) => {
            let _ = write!(out, "unlist {} ;", escape_name(name));
        }

        XfstCommand::Clear => out.push_str("clear ;"),
        XfstCommand::Pop => out.push_str("pop ;"),
        XfstCommand::Push(n) => {
            let _ = write!(out, "push {} ;", escape_name(n));
        }
        XfstCommand::Turn => out.push_str("turn ;"),
        XfstCommand::Rotate => out.push_str("rotate ;"),
        XfstCommand::LoadStack(p) => {
            let _ = write!(out, "load stack {} ;", escape_name(p));
        }
        XfstCommand::LoadDefinitions(p) => {
            let _ = write!(out, "load defined {} ;", escape_name(p));
        }

        XfstCommand::Network(op) => {
            out.push_str(&network_op_str(op));
            out.push_str(" ;");
        }

        XfstCommand::Apply(kind, body) => {
            let head = match kind {
                ApplyKind::Up => "apply up",
                ApplyKind::Down => "apply down",
                ApplyKind::Med => "apply med",
            };
            match body {
                Some(b) if !b.is_empty() => {
                    out.push_str(head);
                    out.push('\n');
                    out.push_str(b);
                    out.push_str("<ctrl-d>");
                }
                _ => {
                    out.push_str(head);
                    out.push_str(" ;");
                }
            }
        }
        XfstCommand::LookupOptimize => out.push_str("lookup-optimize ;"),
        XfstCommand::RemoveOptimization => out.push_str("remove-optimization ;"),

        XfstCommand::Read(cmd) => write_read(out, cmd),
        XfstCommand::Save(cmd) => write_save(out, cmd),
        XfstCommand::Print(cmd) => {
            out.push_str(&print_cmd_str(cmd));
            out.push_str(" ;");
        }
        XfstCommand::Test(kind) => {
            let _ = write!(out, "{} ;", test_kind_str(*kind));
        }

        XfstCommand::Set { var, value } => {
            let _ = write!(out, "set {} {} ;", escape_name(var), value);
        }
        XfstCommand::Show(target) => match target {
            Some(t) => {
                let _ = write!(out, "show {} ;", escape_name(t));
            }
            None => out.push_str("show variables ;"),
        },
        XfstCommand::Echo(t) => {
            let _ = write!(out, "echo {t}");
        }
        XfstCommand::System(t) => {
            let _ = write!(out, "system {t}");
        }
        XfstCommand::Source(p) => {
            let _ = write!(out, "source {} ;", escape_name(p));
        }
        XfstCommand::Quit => out.push_str("quit ;"),

        XfstCommand::Substitute(s) => write_substitute(out, s),

        XfstCommand::Apropos(t) => match t {
            Some(s) => {
                let _ = write!(out, "apropos {s}");
            }
            None => out.push_str("apropos"),
        },
        XfstCommand::Describe(t) => {
            let _ = write!(out, "help {t}");
        }
        XfstCommand::Assert(inner) => {
            out.push_str("assert ");
            write_command(out, &inner.value);
        }
        XfstCommand::AddProps(t) => {
            let _ = write!(out, "add properties {t}");
        }
        XfstCommand::EditProps => out.push_str("edit properties ;"),
        XfstCommand::Hfst(t) => {
            let _ = write!(out, "hfst {t}");
        }
        XfstCommand::For => out.push_str("for"),

        XfstCommand::Redirected { command, redirect } => {
            // Strip the trailing ` ;` (if present) from the inner so we can
            // append the redirect cleanly.
            let mut inner = String::new();
            write_command(&mut inner, &command.value);
            let trimmed = inner.trim_end_matches(';').trim_end();
            out.push_str(trimmed);
            out.push(' ');
            out.push_str(&redirect_str(redirect));
            out.push_str(" ;");
        }
    }
}

fn write_read(out: &mut String, c: &ReadCmd) {
    match c {
        ReadCmd::Text(b) => {
            out.push_str("read text\n");
            out.push_str(b);
            out.push_str("<ctrl-d>");
        }
        ReadCmd::Spaced(b) => {
            out.push_str("read spaced-text\n");
            out.push_str(b);
            out.push_str("<ctrl-d>");
        }
        ReadCmd::Prolog(p) => {
            let _ = write!(out, "read prolog {} ;", escape_name(p));
        }
        ReadCmd::Props(p) => {
            let _ = write!(out, "read properties {} ;", escape_name(p));
        }
        ReadCmd::Lexc(p) => {
            let _ = write!(out, "read lexc {} ;", escape_name(p));
        }
        ReadCmd::Att(p) => {
            let _ = write!(out, "read att {} ;", escape_name(p));
        }
    }
}

fn write_save(out: &mut String, c: &SaveCmd) {
    match c {
        SaveCmd::Stack(p) => {
            let _ = write!(out, "save stack {} ;", escape_name(p));
        }
        SaveCmd::Prolog(p) => {
            let _ = write!(out, "write prolog {} ;", escape_name(p));
        }
        SaveCmd::Spaced(p) => {
            let _ = write!(out, "write spaced-text {} ;", escape_name(p));
        }
        SaveCmd::Text(p) => {
            let _ = write!(out, "write text {} ;", escape_name(p));
        }
        SaveCmd::Dot(p) => {
            let _ = write!(out, "write dot {} ;", escape_name(p));
        }
        SaveCmd::Definition(p) => {
            let _ = write!(out, "write definition {} ;", escape_name(p));
        }
        SaveCmd::Definitions(p) => {
            let _ = write!(out, "write definitions {} ;", escape_name(p));
        }
        SaveCmd::Att(p) => {
            if p.is_empty() {
                out.push_str("write att ;");
            } else {
                let _ = write!(out, "write att {} ;", escape_name(p));
            }
        }
    }
}

fn write_substitute(out: &mut String, s: &SubstituteCmd) {
    match s {
        SubstituteCmd::Symbol { from, to, scope } => {
            out.push_str("substitute symbol");
            for f in from {
                out.push(' ');
                out.push_str(&escape_name(f));
            }
            let _ = write!(out, " for {}", escape_name(to));
            if let Some(sc) = scope {
                let _ = write!(out, " {}", escape_name(sc));
            }
            out.push_str(" ;");
        }
        SubstituteCmd::Label { from, to, scope } => {
            out.push_str("substitute label");
            for f in from {
                out.push(' ');
                out.push_str(&escape_name(f));
            }
            let _ = write!(out, " for {}", escape_name(to));
            if let Some(sc) = scope {
                let _ = write!(out, " {}", escape_name(sc));
            }
            out.push_str(" ;");
        }
        SubstituteCmd::Named { def, label } => {
            let _ = write!(
                out,
                "substitute defined {} {} ;",
                escape_name(def),
                escape_name(label)
            );
        }
    }
}

fn network_op_str(op: &NetworkOp) -> String {
    match op {
        NetworkOp::Compose => "compose net".into(),
        NetworkOp::Concatenate => "concatenate net".into(),
        NetworkOp::Intersect => "intersect net".into(),
        NetworkOp::Union => "union net".into(),
        NetworkOp::Minus => "minus net".into(),
        NetworkOp::Crossproduct => "crossproduct net".into(),
        NetworkOp::Ignore => "ignore net".into(),
        NetworkOp::Invert => "invert net".into(),
        NetworkOp::Reverse => "reverse net".into(),
        NetworkOp::Determinize => "determinize net".into(),
        NetworkOp::Minimize => "minimize net".into(),
        NetworkOp::EpsilonRemove => "epsilon-remove net".into(),
        NetworkOp::PruneNet => "prune net".into(),
        NetworkOp::Negate => "negate net".into(),
        NetworkOp::OnePlus => "one-plus net".into(),
        NetworkOp::ZeroPlus => "zero-plus net".into(),
        NetworkOp::Sort => "sort net".into(),
        NetworkOp::Shuffle => "shuffle net".into(),
        NetworkOp::Substring => "substring net".into(),
        NetworkOp::Cleanup => "cleanup net".into(),
        NetworkOp::Complete => "complete net".into(),
        NetworkOp::LowerSide => "lower-side net".into(),
        NetworkOp::UpperSide => "upper-side net".into(),
        NetworkOp::Sigma => "sigma net".into(),
        NetworkOp::LabelNet => "label net".into(),
        NetworkOp::Inspect => "inspect net".into(),
        NetworkOp::TwosidedFlags => "twosided flag-diacritics".into(),
        NetworkOp::EliminateAll => "eliminate flags".into(),
        NetworkOp::CollectEpsilonLoops => "collect epsilon-loops".into(),
        NetworkOp::CompactSigma => "compact sigma".into(),
        NetworkOp::View => "view net".into(),
        NetworkOp::ExtractAmbiguous => "extract ambiguous".into(),
        NetworkOp::ExtractUnambiguous => "extract unambiguous".into(),
        NetworkOp::Ambiguous => "ambiguous upper".into(),
        NetworkOp::CompileReplaceLower => "compile-replace lower".into(),
        NetworkOp::CompileReplaceUpper => "compile-replace upper".into(),
        NetworkOp::EliminateFlag(name) => format!("eliminate flag {}", escape_name(name)),
        NetworkOp::Name(name) => {
            if name.is_empty() {
                "name net".into()
            } else {
                format!("name net {}", escape_name(name))
            }
        }
    }
}

fn print_cmd_str(c: &PrintCmd) -> String {
    match c {
        PrintCmd::Net => "print net".into(),
        PrintCmd::Stack => "print stack".into(),
        PrintCmd::Sigma => "print sigma".into(),
        PrintCmd::SigmaCount => "print sigma-tally".into(),
        PrintCmd::SigmaWordCount => "print sigma-word-tally".into(),
        PrintCmd::Size => "print size".into(),
        PrintCmd::LongestString => "print longest-string".into(),
        PrintCmd::LongestStringSize => "print longest-string-size".into(),
        PrintCmd::ShortestString => "print shortest-string".into(),
        PrintCmd::ShortestStringSize => "print shortest-string-size".into(),
        PrintCmd::Flags => "print flags".into(),
        PrintCmd::Labels(arg) => match arg {
            Some(a) => format!("print labels {}", escape_name(a)),
            None => "print labels".into(),
        },
        PrintCmd::LabelCount => "print label-tally".into(),
        PrintCmd::LabelMaps => "print label-maps".into(),
        PrintCmd::Name => "print name".into(),
        PrintCmd::Aliases => "print aliases".into(),
        PrintCmd::Arccount => "print arc-tally".into(),
        PrintCmd::Defined => "print defined".into(),
        PrintCmd::Dir => "print directory".into(),
        PrintCmd::FileInfo => "print file-info".into(),
        PrintCmd::List => "print list".into(),
        PrintCmd::Lists => "print lists".into(),
        PrintCmd::Words(n) => print_with_count("print words", *n),
        PrintCmd::LowerWords(n) => print_with_count("print lower-words", *n),
        PrintCmd::UpperWords(n) => print_with_count("print upper-words", *n),
        PrintCmd::RandomWords(n) => print_with_count("print random-words", *n),
        PrintCmd::RandomLower(n) => print_with_count("print random-lower", *n),
        PrintCmd::RandomUpper(n) => print_with_count("print random-upper", *n),
        PrintCmd::Props => "print properties".into(),
    }
}

fn print_with_count(head: &str, n: Option<u32>) -> String {
    match n {
        Some(n) => format!("{head} {n}"),
        None => head.into(),
    }
}

fn test_kind_str(k: TestKind) -> &'static str {
    match k {
        TestKind::Eq => "test equivalent",
        TestKind::Funct => "test functional",
        TestKind::Id => "test identity",
        TestKind::Null => "test null",
        TestKind::Nonnull => "test non-null",
        TestKind::Overlap => "test overlap",
        TestKind::Sublanguage => "test sublanguage",
        TestKind::Unambiguous => "test unambiguous",
        TestKind::InfinitelyAmbiguous => "test infinitely-ambiguous",
        TestKind::LowerBounded => "test lower-bounded",
        TestKind::LowerUni => "test lower-universal",
        TestKind::UpperBounded => "test upper-bounded",
        TestKind::UpperUni => "test upper-universal",
    }
}

fn redirect_str(r: &Redirect) -> String {
    let prefix = match r.kind {
        RedirectKind::In => "<",
        RedirectKind::Out => ">",
        RedirectKind::Append => ">>",
    };
    format!("{prefix}{}", escape_name(&r.path))
}

fn escape_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if needs_escape(c) {
            out.push('%');
        }
        out.push(c);
    }
    out
}

fn needs_escape(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t'
            | '\n'
            | '\r'
            | '<'
            | '>'
            | '('
            | ')'
            | '['
            | ']'
            | '!'
            | ';'
            | ':'
            | '"'
            | '#'
            | ','
            | '%'
    )
}

// ───────────────────────── strip_groups ─────────────────────────

pub fn strip_groups(script: &Spanned<XfstScript>) -> Spanned<XfstScript> {
    let commands = script
        .value
        .commands
        .iter()
        .map(|c| Spanned::new(strip_command(&c.value), c.span.clone()))
        .collect();
    Spanned::new(XfstScript { commands }, script.span.clone())
}

fn strip_command(c: &XfstCommand) -> XfstCommand {
    match c {
        XfstCommand::Regex(body) => XfstCommand::Regex(nfst_xre::strip_groups(body)),
        XfstCommand::Define { name, body } => XfstCommand::Define {
            name: name.clone(),
            body: nfst_xre::strip_groups(body),
        },
        XfstCommand::DefineFunction { name, params, body } => XfstCommand::DefineFunction {
            name: name.clone(),
            params: params.clone(),
            body: nfst_xre::strip_groups(body),
        },
        XfstCommand::Assert(inner) => XfstCommand::Assert(Box::new(Spanned::new(
            strip_command(&inner.value),
            inner.span.clone(),
        ))),
        XfstCommand::Redirected { command, redirect } => XfstCommand::Redirected {
            command: Box::new(Spanned::new(
                strip_command(&command.value),
                command.span.clone(),
            )),
            redirect: redirect.clone(),
        },
        other => other.clone(),
    }
}
