//! Pretty-printer for twolc source. Defensive escaping; round-trip via
//! `strip_groups` to peel `Group` wrappers introduced by precedence-safe
//! bracketing.

use crate::ast::{
    BinaryOp, CenterPair, CenterSide, RuleCenter, RuleContext, RuleOp, SetDefinition,
    TwolcDefinition, TwolcFile, TwolcRegex, TwolcRule, UnaryOp, VarMatcher, VariableAssignment,
    VariableBlock,
};
use nfst_syntax::Spanned;
use smol_str::{SmolStr, SmolStrBuilder};
use std::fmt::Write;

pub fn pretty_print(file: &Spanned<TwolcFile>) -> SmolStr {
    let mut out = SmolStrBuilder::new();
    let f = &file.value;

    if !f.alphabet.is_empty() {
        out.push_str("Alphabet\n");
        for p in &f.alphabet {
            out.push_str("  ");
            out.push_str(&escape_symbol(&p.value.upper));
            if p.value.upper != p.value.lower {
                out.push(':');
                out.push_str(&escape_symbol(&p.value.lower));
            }
            out.push('\n');
        }
        out.push_str("  ;\n\n");
    }

    if !f.diacritics.is_empty() {
        out.push_str("Diacritics\n");
        for d in &f.diacritics {
            out.push_str("  ");
            out.push_str(&escape_symbol(&d.value));
            out.push('\n');
        }
        out.push_str("  ;\n\n");
    }

    if !f.sets.is_empty() {
        out.push_str("Sets\n");
        for s in &f.sets {
            write_set(&mut out, &s.value);
        }
        out.push('\n');
    }

    if !f.definitions.is_empty() {
        out.push_str("Definitions\n");
        for d in &f.definitions {
            write_definition(&mut out, &d.value);
        }
        out.push('\n');
    }

    out.push_str("Rules\n\n");
    for r in &f.rules {
        write_rule(&mut out, &r.value);
        out.push('\n');
    }

    out.finish()
}

fn write_set(out: &mut SmolStrBuilder, s: &SetDefinition) {
    let _ = write!(out, "  {} =", escape_symbol(&s.name));
    for m in &s.members {
        out.push(' ');
        out.push_str(&escape_symbol(m));
    }
    out.push_str(" ;\n");
}

fn write_definition(out: &mut SmolStrBuilder, d: &TwolcDefinition) {
    let _ = write!(out, "  {} = ", escape_symbol(&d.name));
    write_regex(out, &d.body.value);
    out.push_str(" ;\n");
}

fn write_rule(out: &mut SmolStrBuilder, r: &TwolcRule) {
    let _ = write!(out, "\"{}\"\n  ", r.name);
    write_rule_center(out, &r.center);
    out.push(' ');
    out.push_str(rule_op_str(r.operator));
    if r.positive_contexts.is_empty() {
        out.push_str(" ;\n");
    } else {
        for (i, c) in r.positive_contexts.iter().enumerate() {
            if i > 0 {
                out.push_str("    ");
            } else {
                out.push(' ');
            }
            write_rule_context(out, c);
            out.push_str(" ;\n");
        }
    }
    if !r.negative_contexts.is_empty() {
        out.push_str("  except\n");
        for c in &r.negative_contexts {
            out.push_str("    ");
            write_rule_context(out, c);
            out.push_str(" ;\n");
        }
    }
    if let Some(blocks) = &r.variables {
        out.push_str("  where ");
        for (i, b) in blocks.iter().enumerate() {
            if i > 0 {
                out.push_str(" and ");
            }
            write_variable_block(out, b);
        }
        out.push_str(" ;\n");
    }
}

fn write_rule_center(out: &mut SmolStrBuilder, c: &RuleCenter) {
    match c {
        RuleCenter::Pair(pairs) if pairs.len() == 1 => {
            write_center_pair(out, &pairs[0]);
        }
        RuleCenter::Pair(pairs) => {
            out.push('[');
            for (i, p) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push_str(" | ");
                }
                write_center_pair(out, p);
            }
            out.push(']');
        }
        RuleCenter::Regex(body) => {
            out.push_str("<[ ");
            write_regex(out, &body.value);
            out.push_str(" ]>");
        }
    }
}

/// Both sides always written out: the source's elided forms (`a:`, `:b`, `:`)
/// print as their meaning (`a:?`, `?:b`, `?:?`), which re-parses to the same
/// tree. `escape_symbol` writes a literal `?` symbol as `%?`, so it stays
/// distinct from the wildcard on the way back in.
fn write_center_pair(out: &mut SmolStrBuilder, p: &CenterPair) {
    write_center_side(out, &p.upper);
    out.push(':');
    write_center_side(out, &p.lower);
}

fn write_center_side(out: &mut SmolStrBuilder, s: &CenterSide) {
    match s {
        CenterSide::Any => out.push('?'),
        CenterSide::Symbol(sym) => out.push_str(&escape_symbol(sym)),
    }
}

fn write_rule_context(out: &mut SmolStrBuilder, c: &RuleContext) {
    // Empty contexts on either side were truly empty in the source —
    // round-tripping requires preserving that, not synthesising `0`.
    if !matches!(c.left.value, TwolcRegex::Epsilon) {
        write_regex_atom_or_bracketed(out, &c.left.value);
        out.push(' ');
    }
    out.push('_');
    if !matches!(c.right.value, TwolcRegex::Epsilon) {
        out.push(' ');
        write_regex_atom_or_bracketed(out, &c.right.value);
    }
}

fn write_variable_block(out: &mut SmolStrBuilder, b: &VariableBlock) {
    for (i, a) in b.assignments.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        write_variable_assignment(out, a);
    }
    if !matches!(b.matcher, VarMatcher::Freely) {
        out.push(' ');
        out.push_str(matcher_str(b.matcher));
    }
}

fn write_variable_assignment(out: &mut SmolStrBuilder, a: &VariableAssignment) {
    let _ = write!(out, "{} in (", escape_symbol(&a.name));
    for v in &a.values {
        out.push(' ');
        out.push_str(&escape_symbol(v));
    }
    out.push_str(" )");
}

fn matcher_str(m: VarMatcher) -> &'static str {
    match m {
        VarMatcher::Matched => "matched",
        VarMatcher::Mixed => "mixed",
        VarMatcher::Freely => "freely",
    }
}

fn rule_op_str(op: RuleOp) -> &'static str {
    match op {
        RuleOp::Right => "=>",
        RuleOp::Left => "<=",
        RuleOp::LeftRight => "<=>",
        RuleOp::NotLeft => "/<=",
    }
}

// ───────────────────────── regex ─────────────────────────

fn write_regex(out: &mut SmolStrBuilder, e: &TwolcRegex) {
    match e {
        TwolcRegex::Symbol(s) => out.push_str(&escape_symbol(s)),
        TwolcRegex::Epsilon => out.push('0'),
        TwolcRegex::Any => out.push('?'),
        TwolcRegex::Pair { upper, lower } => {
            write_regex_atom_or_bracketed(out, &upper.value);
            out.push(':');
            write_regex_atom_or_bracketed(out, &lower.value);
        }
        TwolcRegex::Group(inner) => {
            out.push('[');
            write_regex(out, &inner.value);
            out.push(']');
        }
        TwolcRegex::Optional(inner) => {
            out.push('(');
            write_regex(out, &inner.value);
            out.push(')');
        }
        TwolcRegex::Binary(op, l, r) => {
            write_regex_atom_or_bracketed(out, &l.value);
            let sep = match op {
                BinaryOp::Concatenate => " ",
                BinaryOp::Union => " | ",
                BinaryOp::Intersect => " & ",
                BinaryOp::Subtract => " - ",
                BinaryOp::Ignoring => " / ",
                _ => " ",
            };
            out.push_str(sep);
            write_regex_atom_or_bracketed(out, &r.value);
        }
        TwolcRegex::Unary(op, inner) => match op {
            UnaryOp::Star => {
                write_regex_atom_or_bracketed(out, &inner.value);
                out.push('*');
            }
            UnaryOp::Plus => {
                write_regex_atom_or_bracketed(out, &inner.value);
                out.push('+');
            }
            UnaryOp::Complement => {
                out.push('~');
                write_regex_atom_or_bracketed(out, &inner.value);
            }
            UnaryOp::TermComplement => {
                out.push('\\');
                write_regex_atom_or_bracketed(out, &inner.value);
            }
            UnaryOp::Containment => {
                out.push('$');
                write_regex_atom_or_bracketed(out, &inner.value);
            }
            UnaryOp::ContainmentOnce => {
                out.push_str("$.");
                write_regex_atom_or_bracketed(out, &inner.value);
            }
            _ => {
                write_regex_atom_or_bracketed(out, &inner.value);
            }
        },
        TwolcRegex::RepeatN(e, n) => {
            write_regex_atom_or_bracketed(out, &e.value);
            let _ = write!(out, "^{n}");
        }
        TwolcRegex::RepeatNToK(e, n, k) => {
            write_regex_atom_or_bracketed(out, &e.value);
            let _ = write!(out, "^{n},{k}");
        }
    }
}

fn write_regex_atom_or_bracketed(out: &mut SmolStrBuilder, e: &TwolcRegex) {
    if is_atomic(e) {
        write_regex(out, e);
    } else {
        out.push('[');
        write_regex(out, e);
        out.push(']');
    }
}

fn is_atomic(e: &TwolcRegex) -> bool {
    matches!(
        e,
        TwolcRegex::Symbol(_)
            | TwolcRegex::Epsilon
            | TwolcRegex::Any
            | TwolcRegex::Group(_)
            | TwolcRegex::Optional(_)
            | TwolcRegex::Pair { .. }
    )
}

/// Escape any character special in twolc's lexer by prefixing with `%`.
///
/// The lexer renames the BARE special tokens (`0`, `#`, `.#.`) into the
/// `__HFST_TWOLC_` namespace and strips `%`-escapes, so printing must invert
/// both directions for the round-trip to hold: a marker prints as its bare
/// spelling, and a plain symbol that WOULD lex into a marker prints escaped.
fn escape_symbol(s: &str) -> SmolStr {
    match s {
        "__HFST_TWOLC_0" => return SmolStr::new_static("0"),
        "__HFST_TWOLC_#" => return SmolStr::new_static("#"),
        "__HFST_TWOLC_.#." => return SmolStr::new_static(".#."),
        "0" => return SmolStr::new_static("%0"),
        "#" => return SmolStr::new_static("%#"),
        ".#." => return SmolStr::new_static(".%#."),
        _ => {}
    }
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
    matches!(
        c,
        '*' | '+'
            | '/'
            | '\\'
            | '='
            | '"'
            | '$'
            | '?'
            | '|'
            | '&'
            | '^'
            | '-'
            | '{'
            | '}'
            | '['
            | ']'
            | '('
            | ')'
            | ':'
            | ';'
            | '_'
            | '!'
            | '%'
            | '~'
            | ' '
            | '\t'
            | '\r'
            | '\n'
    )
}

// ───────────────────────── strip_groups ─────────────────────────

pub fn strip_groups(file: &Spanned<TwolcFile>) -> Spanned<TwolcFile> {
    let f = &file.value;
    Spanned::new(
        TwolcFile {
            alphabet: f.alphabet.clone(),
            diacritics: f.diacritics.clone(),
            sets: f.sets.clone(),
            definitions: f
                .definitions
                .iter()
                .map(|d| {
                    Spanned::new(
                        TwolcDefinition {
                            name: d.value.name.clone(),
                            body: strip_regex(&d.value.body),
                        },
                        d.span.clone(),
                    )
                })
                .collect(),
            rules: f
                .rules
                .iter()
                .map(|r| {
                    Spanned::new(
                        TwolcRule {
                            name: r.value.name.clone(),
                            center: strip_center(&r.value.center),
                            operator: r.value.operator,
                            positive_contexts: r
                                .value
                                .positive_contexts
                                .iter()
                                .map(strip_context)
                                .collect(),
                            negative_contexts: r
                                .value
                                .negative_contexts
                                .iter()
                                .map(strip_context)
                                .collect(),
                            variables: r.value.variables.clone(),
                        },
                        r.span.clone(),
                    )
                })
                .collect(),
        },
        file.span.clone(),
    )
}

fn strip_center(c: &RuleCenter) -> RuleCenter {
    match c {
        RuleCenter::Pair(p) => RuleCenter::Pair(p.clone()),
        RuleCenter::Regex(b) => RuleCenter::Regex(Box::new(strip_regex(b))),
    }
}

fn strip_context(c: &RuleContext) -> RuleContext {
    RuleContext {
        left: strip_regex(&c.left),
        right: strip_regex(&c.right),
    }
}

fn strip_regex(e: &Spanned<TwolcRegex>) -> Spanned<TwolcRegex> {
    let span = e.span.clone();
    let value = match &e.value {
        TwolcRegex::Group(inner) => return strip_regex(inner),
        TwolcRegex::Symbol(s) => TwolcRegex::Symbol(s.clone()),
        TwolcRegex::Epsilon => TwolcRegex::Epsilon,
        TwolcRegex::Any => TwolcRegex::Any,
        TwolcRegex::Pair { upper, lower } => TwolcRegex::Pair {
            upper: Box::new(strip_regex(upper)),
            lower: Box::new(strip_regex(lower)),
        },
        TwolcRegex::Optional(x) => TwolcRegex::Optional(Box::new(strip_regex(x))),
        TwolcRegex::Binary(op, l, r) => {
            TwolcRegex::Binary(*op, Box::new(strip_regex(l)), Box::new(strip_regex(r)))
        }
        TwolcRegex::Unary(op, x) => TwolcRegex::Unary(*op, Box::new(strip_regex(x))),
        TwolcRegex::RepeatN(x, n) => TwolcRegex::RepeatN(Box::new(strip_regex(x)), *n),
        TwolcRegex::RepeatNToK(x, n, k) => TwolcRegex::RepeatNToK(Box::new(strip_regex(x)), *n, *k),
    };
    Spanned::new(value, span)
}
