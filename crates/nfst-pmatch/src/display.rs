//! Pretty-printer for pmatch source. Produces parseable text from a
//! `Spanned<PmatchFile>`. Defensive bracketing keeps the output safe to
//! re-parse; round-trip tests strip `Group` wrappers via `strip_groups`.

use crate::ast::{
    Acceptor, BinaryOp, CaseOp, CaseSide, ContextMark, MappingKind, MappingPair, MappingSide,
    PmatchExpr, PmatchFile, PmatchReplaceRule, PmatchStatement, ReadKind, ReplaceArrow,
    ReplaceContext, ReplaceContexts, RestrContext, SpannedExpr, UnaryOp, VariableValue,
};
use nfst_syntax::Spanned;
use smol_str::{SmolStr, SmolStrBuilder};
use std::fmt::Write;

pub fn pretty_print(file: &Spanned<PmatchFile>) -> SmolStr {
    let mut out = SmolStrBuilder::new();
    for stmt in &file.value.statements {
        write_statement(&mut out, &stmt.value);
    }
    out.finish()
}

fn write_statement(out: &mut SmolStrBuilder, st: &PmatchStatement) {
    match st {
        PmatchStatement::Define { name, params, body } => {
            out.push_str("Define ");
            out.push_str(name);
            if let Some(args) = params {
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(a);
                }
                out.push(')');
            }
            out.push(' ');
            write_expr(out, &body.value);
            out.push_str(" ;\n");
        }
        PmatchStatement::DefIns { name, body } => {
            let _ = write!(out, "DefIns {name} ");
            write_expr(out, &body.value);
            out.push_str(" ;\n");
        }
        PmatchStatement::RegexTop { body } => {
            out.push_str("regex ");
            write_expr(out, &body.value);
            out.push_str(" ;\n");
        }
        PmatchStatement::SetVariable { name, value } => {
            let _ = write!(out, "set {name} ");
            match value {
                VariableValue::Symbol(s) => out.push_str(s),
                VariableValue::Epsilon => out.push('0'),
            }
            out.push('\n');
        }
        PmatchStatement::ListDefinition { name, body } => {
            let _ = write!(out, "list {name} ");
            write_expr(out, &body.value);
            out.push_str(" ;\n");
        }
        PmatchStatement::ReadVec { path } => {
            let _ = writeln!(out, "@vec\"{path}\"");
        }
    }
}

fn write_expr(out: &mut SmolStrBuilder, e: &PmatchExpr) {
    match e {
        PmatchExpr::Symbol(s) => out.push_str(&escape_symbol(s)),
        PmatchExpr::Literal(s) => {
            let _ = write!(out, "Lit({s})");
        }
        PmatchExpr::QuotedLiteral(s) => {
            out.push('"');
            out.push_str(s);
            out.push('"');
        }
        PmatchExpr::CurlyLiteral(s) => {
            out.push('{');
            out.push_str(s);
            out.push('}');
        }
        PmatchExpr::Epsilon => out.push('0'),
        PmatchExpr::Any => out.push('?'),
        PmatchExpr::BoundaryMarker => out.push('#'),
        PmatchExpr::Acceptor(a) => out.push_str(acceptor_name(*a)),
        PmatchExpr::CharacterRange { from, to } => {
            let _ = write!(out, "\"{from}-{to}\"");
        }
        PmatchExpr::Binary(op, l, r) => {
            write_atom_or_bracketed(out, &l.value);
            out.push(' ');
            out.push_str(binary_op_str(*op));
            out.push(' ');
            write_atom_or_bracketed(out, &r.value);
        }
        PmatchExpr::Unary(op, inner) => write_unary(out, *op, &inner.value),
        PmatchExpr::Group(inner) => {
            out.push('[');
            write_expr(out, &inner.value);
            out.push(']');
        }
        PmatchExpr::Optional(inner) => {
            out.push('(');
            write_expr(out, &inner.value);
            out.push(')');
        }
        PmatchExpr::BracketedDotted(None) => out.push_str("[..]"),
        PmatchExpr::BracketedDotted(Some(inner)) => {
            out.push_str("[. ");
            write_expr(out, &inner.value);
            out.push_str(" .]");
        }
        PmatchExpr::Pair { upper, lower } => {
            write_atom_or_bracketed(out, &upper.value);
            out.push(':');
            write_atom_or_bracketed(out, &lower.value);
        }
        PmatchExpr::Weighted { expr, weight } => {
            write_atom_or_bracketed(out, &expr.value);
            let _ = write!(out, "::{weight}");
        }
        PmatchExpr::RepeatN(e, n) => {
            write_atom_or_bracketed(out, &e.value);
            let _ = write!(out, "^{n}");
        }
        PmatchExpr::RepeatNPlus(e, n) => {
            write_atom_or_bracketed(out, &e.value);
            let _ = write!(out, "^>{n}");
        }
        PmatchExpr::RepeatNMinus(e, n) => {
            write_atom_or_bracketed(out, &e.value);
            let _ = write!(out, "^<{n}");
        }
        PmatchExpr::RepeatNToK(e, n, k) => {
            write_atom_or_bracketed(out, &e.value);
            let _ = write!(out, "^{n},{k}");
        }
        PmatchExpr::Replace { arrow, rules } => write_replace(out, *arrow, rules),
        PmatchExpr::Restriction { body, contexts } => {
            write_atom_or_bracketed(out, &body.value);
            out.push_str(" => ");
            for (i, c) in contexts.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_restr_context(out, c);
            }
        }
        PmatchExpr::Ins(s) => {
            let _ = write!(out, "Ins({s})");
        }
        PmatchExpr::EndTag(s) => {
            let _ = write!(out, "EndTag({s})");
        }
        PmatchExpr::Capture(s) => {
            let _ = write!(out, "Capture({s})");
        }
        PmatchExpr::Counter(s) => {
            let _ = write!(out, "Counter({s})");
        }
        PmatchExpr::Tag { body, name } => {
            out.push('[');
            write_expr(out, &body.value);
            let _ = write!(out, "].t({name})");
        }
        PmatchExpr::With { body, name, value } => {
            out.push('[');
            write_expr(out, &body.value);
            let _ = write!(out, "].with({name} = {value})");
        }
        PmatchExpr::CaseOp { op, side, body } => {
            out.push_str(case_op_left(*op));
            write_expr(out, &body.value);
            if let Some(s) = side {
                out.push_str(", ");
                out.push(match s {
                    CaseSide::Upper => 'U',
                    CaseSide::Lower => 'L',
                });
            }
            out.push(')');
        }
        PmatchExpr::DefineWrapper(inner) => {
            out.push_str("Define(");
            write_expr(out, &inner.value);
            out.push(')');
        }
        PmatchExpr::Explode(items) => write_call_list(out, "Explode(", items),
        PmatchExpr::Implode(items) => write_call_list(out, "Implode(", items),
        PmatchExpr::Like {
            args,
            threshold,
            unlike,
        } => {
            out.push_str(if *unlike { "Unlike(" } else { "Like(" });
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(a);
            }
            out.push(')');
            if let Some(t) = threshold {
                let _ = write!(out, "^{t}");
            }
        }
        PmatchExpr::Lst(inner) => {
            out.push_str("Lst(");
            write_expr(out, &inner.value);
            out.push(')');
        }
        PmatchExpr::Exc(inner) => {
            out.push_str("Exc(");
            write_expr(out, &inner.value);
            out.push(')');
        }
        PmatchExpr::Sigma(inner) => {
            out.push_str("Sigma(");
            write_expr(out, &inner.value);
            out.push(')');
        }
        PmatchExpr::Interpolate(items) => write_call_list(out, "Interpolate(", items),
        PmatchExpr::Substitute(a, b, c) => {
            out.push_str("`[ ");
            write_expr(out, &a.value);
            out.push_str(" , ");
            write_expr(out, &b.value);
            out.push_str(" , ");
            write_expr(out, &c.value);
            out.push_str(" ]");
        }
        PmatchExpr::Uncompose(a, b, c) => {
            out.push_str("Uncompose(");
            write_expr(out, &a.value);
            out.push_str(", ");
            write_expr(out, &b.value);
            out.push_str(", ");
            write_expr(out, &c.value);
            out.push(')');
        }
        PmatchExpr::Lc(inner) => {
            out.push_str("LC(");
            write_expr(out, &inner.value);
            out.push(')');
        }
        PmatchExpr::Rc(inner) => {
            out.push_str("RC(");
            write_expr(out, &inner.value);
            out.push(')');
        }
        PmatchExpr::Nlc(inner) => {
            out.push_str("NLC(");
            write_expr(out, &inner.value);
            out.push(')');
        }
        PmatchExpr::Nrc(inner) => {
            out.push_str("NRC(");
            write_expr(out, &inner.value);
            out.push(')');
        }
        PmatchExpr::OrContext(items) => write_call_list(out, "OR(", items),
        PmatchExpr::AndContext(items) => write_call_list(out, "AND(", items),
        PmatchExpr::Call { name, args } => {
            let _ = write!(out, "{name}(");
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_expr(out, &a.value);
            }
            out.push(')');
        }
        PmatchExpr::ReadFile { kind, path } => {
            let prefix = match kind {
                ReadKind::Binary => "@bin",
                ReadKind::Text => "@txt",
                ReadKind::Spaced => "@stxt",
                ReadKind::Prolog => "@pl",
                ReadKind::Regex => "@re",
            };
            let _ = write!(out, "{prefix}\"{path}\"");
        }
        PmatchExpr::ReadLexc(p) => {
            let _ = write!(out, "@lexc\"{p}\"");
        }
        PmatchExpr::ReadVec(p) => {
            let _ = write!(out, "@vec\"{p}\"");
        }
    }
}

fn write_call_list(out: &mut SmolStrBuilder, prefix: &str, items: &[SpannedExpr]) {
    out.push_str(prefix);
    for (i, it) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_expr(out, &it.value);
    }
    out.push(')');
}

fn write_atom_or_bracketed(out: &mut SmolStrBuilder, e: &PmatchExpr) {
    if is_atomic(e) {
        write_expr(out, e);
    } else {
        out.push('[');
        write_expr(out, e);
        out.push(']');
    }
}

fn is_atomic(e: &PmatchExpr) -> bool {
    matches!(
        e,
        PmatchExpr::Symbol(_)
            | PmatchExpr::Literal(_)
            | PmatchExpr::QuotedLiteral(_)
            | PmatchExpr::CurlyLiteral(_)
            | PmatchExpr::Epsilon
            | PmatchExpr::Any
            | PmatchExpr::BoundaryMarker
            | PmatchExpr::Acceptor(_)
            | PmatchExpr::CharacterRange { .. }
            | PmatchExpr::Group(_)
            | PmatchExpr::Optional(_)
            | PmatchExpr::BracketedDotted(_)
            | PmatchExpr::Ins(_)
            | PmatchExpr::EndTag(_)
            | PmatchExpr::Capture(_)
            | PmatchExpr::Counter(_)
            | PmatchExpr::Tag { .. }
            | PmatchExpr::With { .. }
            | PmatchExpr::CaseOp { .. }
            | PmatchExpr::DefineWrapper(_)
            | PmatchExpr::Explode(_)
            | PmatchExpr::Implode(_)
            | PmatchExpr::Like { .. }
            | PmatchExpr::Lst(_)
            | PmatchExpr::Exc(_)
            | PmatchExpr::Sigma(_)
            | PmatchExpr::Interpolate(_)
            | PmatchExpr::Substitute(_, _, _)
            | PmatchExpr::Uncompose(_, _, _)
            | PmatchExpr::Lc(_)
            | PmatchExpr::Rc(_)
            | PmatchExpr::Nlc(_)
            | PmatchExpr::Nrc(_)
            | PmatchExpr::OrContext(_)
            | PmatchExpr::AndContext(_)
            | PmatchExpr::Call { .. }
            | PmatchExpr::ReadFile { .. }
            | PmatchExpr::ReadLexc(_)
            | PmatchExpr::ReadVec(_)
    )
}

fn write_unary(out: &mut SmolStrBuilder, op: UnaryOp, inner: &PmatchExpr) {
    match op {
        UnaryOp::Star => {
            write_atom_or_bracketed(out, inner);
            out.push('*');
        }
        UnaryOp::Plus => {
            write_atom_or_bracketed(out, inner);
            out.push('+');
        }
        UnaryOp::Reverse => {
            write_atom_or_bracketed(out, inner);
            out.push_str(".r");
        }
        UnaryOp::Invert => {
            write_atom_or_bracketed(out, inner);
            out.push_str(".i");
        }
        UnaryOp::UpperProject => {
            write_atom_or_bracketed(out, inner);
            out.push_str(".u");
        }
        UnaryOp::LowerProject => {
            write_atom_or_bracketed(out, inner);
            out.push_str(".l");
        }
        UnaryOp::Complement => {
            out.push('~');
            write_atom_or_bracketed(out, inner);
        }
        UnaryOp::TermComplement => {
            out.push('\\');
            write_atom_or_bracketed(out, inner);
        }
        UnaryOp::Containment => {
            out.push('$');
            write_atom_or_bracketed(out, inner);
        }
        UnaryOp::ContainmentOnce => {
            out.push_str("$.");
            write_atom_or_bracketed(out, inner);
        }
        UnaryOp::ContainmentOpt => {
            out.push_str("$?");
            write_atom_or_bracketed(out, inner);
        }
    }
}

fn write_replace(out: &mut SmolStrBuilder, arrow: ReplaceArrow, rules: &[PmatchReplaceRule]) {
    let arrow_str = match arrow {
        ReplaceArrow::Right => "->",
        ReplaceArrow::OptionalRight => "(->)",
        ReplaceArrow::Left => "<-",
        ReplaceArrow::OptionalLeft => "(<-)",
        ReplaceArrow::LeftRight => "<->",
        ReplaceArrow::OptionalLeftRight => "(<->)",
        ReplaceArrow::LtrLongest => "@->",
        ReplaceArrow::LtrShortest => "@>",
        ReplaceArrow::RtlLongest => "->@",
        ReplaceArrow::RtlShortest => ">@",
    };
    for (i, rule) in rules.iter().enumerate() {
        if i > 0 {
            out.push_str(" ,, ");
        }
        for (j, m) in rule.mappings.iter().enumerate() {
            if j > 0 {
                out.push_str(" , ");
            }
            write_mapping_pair(out, m, arrow_str);
        }
        if let Some(cx) = &rule.contexts {
            out.push(' ');
            out.push_str(context_mark_str(cx.mark));
            for (k, item) in cx.items.iter().enumerate() {
                if k > 0 {
                    out.push_str(" , ");
                }
                out.push(' ');
                write_replace_context(out, item);
            }
        }
    }
}

fn write_mapping_pair(out: &mut SmolStrBuilder, m: &MappingPair, arrow_str: &str) {
    write_mapping_side(out, &m.upper);
    let _ = write!(out, " {arrow_str} ");
    match &m.kind {
        MappingKind::Plain { lower } => write_mapping_side(out, lower),
        MappingKind::Markup { pre, post } => {
            if let Some(pre) = pre {
                write_mapping_side(out, pre);
                out.push(' ');
            }
            out.push_str("...");
            if let Some(post) = post {
                out.push(' ');
                write_mapping_side(out, post);
            }
        }
    }
}

fn write_mapping_side(out: &mut SmolStrBuilder, side: &MappingSide) {
    match side {
        MappingSide::Expr(b) => write_atom_or_bracketed(out, &b.value),
        MappingSide::Dotted(None) => out.push_str("[..]"),
        MappingSide::Dotted(Some(b)) => {
            out.push_str("[. ");
            write_expr(out, &b.value);
            out.push_str(" .]");
        }
    }
}

fn write_replace_context(out: &mut SmolStrBuilder, c: &ReplaceContext) {
    if let Some(left) = &c.left {
        write_atom_or_bracketed(out, &left.value);
        out.push(' ');
    }
    out.push('_');
    if let Some(right) = &c.right {
        out.push(' ');
        write_atom_or_bracketed(out, &right.value);
    }
}

fn write_restr_context(out: &mut SmolStrBuilder, c: &RestrContext) {
    if let Some(left) = &c.left {
        write_atom_or_bracketed(out, &left.value);
        out.push(' ');
    }
    out.push('_');
    if let Some(right) = &c.right {
        out.push(' ');
        write_atom_or_bracketed(out, &right.value);
    }
}

fn binary_op_str(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Concatenate => "",
        BinaryOp::Compose => ".o.",
        BinaryOp::LenientCompose => ".O.",
        BinaryOp::CrossProduct => ".x.",
        BinaryOp::MergeRight => ".m>.",
        BinaryOp::MergeLeft => ".<m.",
        BinaryOp::Before => "<",
        BinaryOp::After => ">",
        BinaryOp::Shuffle => "<>",
        BinaryOp::Union => "|",
        BinaryOp::Intersect => "&",
        BinaryOp::Subtract => "-",
        BinaryOp::UpperSubtract => ".-u.",
        BinaryOp::LowerSubtract => ".-l.",
        BinaryOp::UpperPriorityUnion => ".P.",
        BinaryOp::LowerPriorityUnion => ".p.",
        BinaryOp::Ignoring => "/",
        BinaryOp::IgnoreInternally => "./.",
        BinaryOp::LeftQuotient => "\\\\\\",
    }
}

fn context_mark_str(m: ContextMark) -> &'static str {
    match m {
        ContextMark::UpperUpper => "||",
        ContextMark::LowerUpper => "//",
        ContextMark::UpperLower => "\\\\",
        ContextMark::LowerLower => "\\//",
    }
}

fn acceptor_name(a: Acceptor) -> &'static str {
    match a {
        Acceptor::Alpha => "Alpha",
        Acceptor::UppercaseAlpha => "UppercaseAlpha",
        Acceptor::LowercaseAlpha => "LowercaseAlpha",
        Acceptor::Num => "Num",
        Acceptor::Punct => "Punct",
        Acceptor::Whitespace => "Whitespace",
    }
}

/// Insert `%` before any byte that is special in the pmatch lexer.
/// Mirrors the inverse of the lexer's percent-strip step.
fn escape_symbol(s: &str) -> SmolStr {
    let mut out = SmolStrBuilder::new();
    for c in s.chars() {
        if needs_escape(c) {
            out.push('%');
        }
        out.push(c);
    }
    out.finish()
}

fn needs_escape(c: char) -> bool {
    // pmatch's A7UNRESTRICTED exclusion set, plus `%` itself.
    matches!(
        c,
        '-' | ' '
            | '\t'
            | '\r'
            | '\n'
            | '|'
            | '<'
            | '>'
            | '%'
            | '^'
            | ':'
            | ';'
            | ','
            | '@'
            | '~'
            | '\\'
            | '&'
            | '?'
            | '$'
            | '+'
            | '*'
            | '/'
            | '('
            | ')'
            | '{'
            | '}'
            | ']'
            | '['
    )
}

fn case_op_left(op: CaseOp) -> &'static str {
    match op {
        CaseOp::Cap => "Cap(",
        CaseOp::OptCap => "OptCap(",
        CaseOp::ToLower => "DownCase(",
        CaseOp::ToUpper => "UpCase(",
        CaseOp::OptToLower => "OptDownCase(",
        CaseOp::OptToUpper => "OptUpCase(",
        CaseOp::AnyCase => "AnyCase(",
    }
}

// ───────────────────────── strip_groups ─────────────────────────

pub fn strip_groups(file: &Spanned<PmatchFile>) -> Spanned<PmatchFile> {
    let span = file.span.clone();
    let stmts = file
        .value
        .statements
        .iter()
        .map(|s| Spanned::new(strip_stmt(&s.value), s.span.clone()))
        .collect();
    Spanned::new(PmatchFile { statements: stmts }, span)
}

fn strip_stmt(s: &PmatchStatement) -> PmatchStatement {
    match s {
        PmatchStatement::Define { name, params, body } => PmatchStatement::Define {
            name: name.clone(),
            params: params.clone(),
            body: strip_expr(body),
        },
        PmatchStatement::DefIns { name, body } => PmatchStatement::DefIns {
            name: name.clone(),
            body: strip_expr(body),
        },
        PmatchStatement::RegexTop { body } => PmatchStatement::RegexTop {
            body: strip_expr(body),
        },
        PmatchStatement::SetVariable { name, value } => PmatchStatement::SetVariable {
            name: name.clone(),
            value: value.clone(),
        },
        PmatchStatement::ListDefinition { name, body } => PmatchStatement::ListDefinition {
            name: name.clone(),
            body: strip_expr(body),
        },
        PmatchStatement::ReadVec { path } => PmatchStatement::ReadVec { path: path.clone() },
    }
}

fn strip_expr(e: &SpannedExpr) -> SpannedExpr {
    let span = e.span.clone();
    let value = match &e.value {
        PmatchExpr::Group(inner) => return strip_expr(inner),
        PmatchExpr::Symbol(s) => PmatchExpr::Symbol(s.clone()),
        PmatchExpr::Literal(s) => PmatchExpr::Literal(s.clone()),
        PmatchExpr::QuotedLiteral(s) => PmatchExpr::QuotedLiteral(s.clone()),
        PmatchExpr::CurlyLiteral(s) => PmatchExpr::CurlyLiteral(s.clone()),
        PmatchExpr::Epsilon => PmatchExpr::Epsilon,
        PmatchExpr::Any => PmatchExpr::Any,
        PmatchExpr::BoundaryMarker => PmatchExpr::BoundaryMarker,
        PmatchExpr::Acceptor(a) => PmatchExpr::Acceptor(*a),
        PmatchExpr::CharacterRange { from, to } => PmatchExpr::CharacterRange {
            from: from.clone(),
            to: to.clone(),
        },
        PmatchExpr::Binary(op, l, r) => {
            PmatchExpr::Binary(*op, Box::new(strip_expr(l)), Box::new(strip_expr(r)))
        }
        PmatchExpr::Unary(op, x) => PmatchExpr::Unary(*op, Box::new(strip_expr(x))),
        PmatchExpr::Optional(x) => PmatchExpr::Optional(Box::new(strip_expr(x))),
        PmatchExpr::BracketedDotted(o) => {
            PmatchExpr::BracketedDotted(o.as_ref().map(|b| Box::new(strip_expr(b))))
        }
        PmatchExpr::Pair { upper, lower } => PmatchExpr::Pair {
            upper: Box::new(strip_expr(upper)),
            lower: Box::new(strip_expr(lower)),
        },
        PmatchExpr::Weighted { expr, weight } => PmatchExpr::Weighted {
            expr: Box::new(strip_expr(expr)),
            weight: *weight,
        },
        PmatchExpr::RepeatN(x, n) => PmatchExpr::RepeatN(Box::new(strip_expr(x)), *n),
        PmatchExpr::RepeatNPlus(x, n) => PmatchExpr::RepeatNPlus(Box::new(strip_expr(x)), *n),
        PmatchExpr::RepeatNMinus(x, n) => PmatchExpr::RepeatNMinus(Box::new(strip_expr(x)), *n),
        PmatchExpr::RepeatNToK(x, n, k) => PmatchExpr::RepeatNToK(Box::new(strip_expr(x)), *n, *k),
        PmatchExpr::Replace { arrow, rules } => PmatchExpr::Replace {
            arrow: *arrow,
            rules: rules.iter().map(strip_replace_rule).collect(),
        },
        PmatchExpr::Restriction { body, contexts } => PmatchExpr::Restriction {
            body: Box::new(strip_expr(body)),
            contexts: contexts
                .iter()
                .map(|c| RestrContext {
                    left: c.left.as_ref().map(|b| Box::new(strip_expr(b))),
                    right: c.right.as_ref().map(|b| Box::new(strip_expr(b))),
                })
                .collect(),
        },
        PmatchExpr::Ins(s) => PmatchExpr::Ins(s.clone()),
        PmatchExpr::EndTag(s) => PmatchExpr::EndTag(s.clone()),
        PmatchExpr::Capture(s) => PmatchExpr::Capture(s.clone()),
        PmatchExpr::Counter(s) => PmatchExpr::Counter(s.clone()),
        PmatchExpr::Tag { body, name } => PmatchExpr::Tag {
            body: Box::new(strip_expr(body)),
            name: name.clone(),
        },
        PmatchExpr::With { body, name, value } => PmatchExpr::With {
            body: Box::new(strip_expr(body)),
            name: name.clone(),
            value: value.clone(),
        },
        PmatchExpr::CaseOp { op, side, body } => PmatchExpr::CaseOp {
            op: *op,
            side: *side,
            body: Box::new(strip_expr(body)),
        },
        PmatchExpr::DefineWrapper(x) => PmatchExpr::DefineWrapper(Box::new(strip_expr(x))),
        PmatchExpr::Explode(items) => PmatchExpr::Explode(items.iter().map(strip_expr).collect()),
        PmatchExpr::Implode(items) => PmatchExpr::Implode(items.iter().map(strip_expr).collect()),
        PmatchExpr::Like {
            args,
            threshold,
            unlike,
        } => PmatchExpr::Like {
            args: args.clone(),
            threshold: *threshold,
            unlike: *unlike,
        },
        PmatchExpr::Lst(x) => PmatchExpr::Lst(Box::new(strip_expr(x))),
        PmatchExpr::Exc(x) => PmatchExpr::Exc(Box::new(strip_expr(x))),
        PmatchExpr::Sigma(x) => PmatchExpr::Sigma(Box::new(strip_expr(x))),
        PmatchExpr::Interpolate(items) => {
            PmatchExpr::Interpolate(items.iter().map(strip_expr).collect())
        }
        PmatchExpr::Substitute(a, b, c) => PmatchExpr::Substitute(
            Box::new(strip_expr(a)),
            Box::new(strip_expr(b)),
            Box::new(strip_expr(c)),
        ),
        PmatchExpr::Uncompose(a, b, c) => PmatchExpr::Uncompose(
            Box::new(strip_expr(a)),
            Box::new(strip_expr(b)),
            Box::new(strip_expr(c)),
        ),
        PmatchExpr::Lc(x) => PmatchExpr::Lc(Box::new(strip_expr(x))),
        PmatchExpr::Rc(x) => PmatchExpr::Rc(Box::new(strip_expr(x))),
        PmatchExpr::Nlc(x) => PmatchExpr::Nlc(Box::new(strip_expr(x))),
        PmatchExpr::Nrc(x) => PmatchExpr::Nrc(Box::new(strip_expr(x))),
        PmatchExpr::OrContext(items) => {
            PmatchExpr::OrContext(items.iter().map(strip_expr).collect())
        }
        PmatchExpr::AndContext(items) => {
            PmatchExpr::AndContext(items.iter().map(strip_expr).collect())
        }
        PmatchExpr::Call { name, args } => PmatchExpr::Call {
            name: name.clone(),
            args: args.iter().map(strip_expr).collect(),
        },
        PmatchExpr::ReadFile { kind, path } => PmatchExpr::ReadFile {
            kind: *kind,
            path: path.clone(),
        },
        PmatchExpr::ReadLexc(p) => PmatchExpr::ReadLexc(p.clone()),
        PmatchExpr::ReadVec(p) => PmatchExpr::ReadVec(p.clone()),
    };
    Spanned::new(value, span)
}

fn strip_replace_rule(r: &PmatchReplaceRule) -> PmatchReplaceRule {
    PmatchReplaceRule {
        mappings: r
            .mappings
            .iter()
            .map(|m| MappingPair {
                upper: strip_mapping_side(&m.upper),
                kind: match &m.kind {
                    MappingKind::Plain { lower } => MappingKind::Plain {
                        lower: strip_mapping_side(lower),
                    },
                    MappingKind::Markup { pre, post } => MappingKind::Markup {
                        pre: pre.as_ref().map(strip_mapping_side),
                        post: post.as_ref().map(strip_mapping_side),
                    },
                },
            })
            .collect(),
        contexts: r.contexts.as_ref().map(|cx| ReplaceContexts {
            mark: cx.mark,
            items: cx
                .items
                .iter()
                .map(|c| ReplaceContext {
                    left: c.left.as_ref().map(|b| Box::new(strip_expr(b))),
                    right: c.right.as_ref().map(|b| Box::new(strip_expr(b))),
                })
                .collect(),
        }),
    }
}

fn strip_mapping_side(m: &MappingSide) -> MappingSide {
    match m {
        MappingSide::Expr(b) => MappingSide::Expr(Box::new(strip_expr(b))),
        MappingSide::Dotted(o) => MappingSide::Dotted(o.as_ref().map(|b| Box::new(strip_expr(b)))),
    }
}
