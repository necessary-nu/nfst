//! Pretty-printer producing canonical, parseable xre source from an
//! `XreExpr` tree.
//!
//! Strategy: defensive bracketing. Any compound subexpression appearing as a
//! child of another operator is wrapped in `[...]` so the pretty-printer can
//! never get precedence wrong. Round-trip tests use [`strip_groups`] to peel
//! those `Group` nodes back off after re-parsing.
//!
//! Symbols are escaped on the way out: every character that is special in
//! the upstream `xre_lex.ll` (see `A7UNRESTRICTED`) is prefixed with `%`.

use crate::ast::{
    BinaryOp, ContextMark, MappingKind, MappingPair, MappingSide, ReadKind, ReplaceArrow,
    ReplaceContext, ReplaceContexts, ReplaceRule, RestrContext, SpannedXre, SubstituteWhat,
    UnaryOp, XreExpr,
};
use nfst_syntax::Spanned;
use smol_str::{SmolStr, SmolStrBuilder};
use std::fmt::Write;

/// Render a parseable xre source string for the given AST.
pub fn pretty_print(expr: &SpannedXre) -> SmolStr {
    let mut out = SmolStrBuilder::new();
    write_top(&mut out, &expr.value);
    out.finish()
}

/// Strip every `Group` wrapper from the tree. Used by round-trip tests:
/// the pretty-printer adds defensive `[...]` around compound children, which
/// re-parse as `Group(_)`. Removing them yields an AST shape directly
/// comparable to the original.
pub fn strip_groups(expr: &SpannedXre) -> SpannedXre {
    let span = expr.span.clone();
    let value = match &expr.value {
        XreExpr::Group(inner) => return strip_groups(inner),
        XreExpr::Symbol(s) => XreExpr::Symbol(s.clone()),
        XreExpr::Curly(s) => XreExpr::Curly(s.clone()),
        XreExpr::Epsilon => XreExpr::Epsilon,
        XreExpr::Any => XreExpr::Any,
        XreExpr::BoundaryMarker => XreExpr::BoundaryMarker,
        XreExpr::Pair { upper, lower } => XreExpr::Pair {
            upper: Box::new(strip_groups(upper)),
            lower: Box::new(strip_groups(lower)),
        },
        XreExpr::Weighted { expr, weight } => XreExpr::Weighted {
            expr: Box::new(strip_groups(expr)),
            weight: *weight,
        },
        XreExpr::ReadFile { kind, path } => XreExpr::ReadFile {
            kind: *kind,
            path: path.clone(),
        },
        XreExpr::FunctionCall { name, args } => XreExpr::FunctionCall {
            name: name.clone(),
            args: args.iter().map(strip_groups).collect(),
        },
        XreExpr::Optional(inner) => XreExpr::Optional(Box::new(strip_groups(inner))),
        XreExpr::BracketedDotted(inner) => {
            XreExpr::BracketedDotted(inner.as_ref().map(|b| Box::new(strip_groups(b))))
        }
        XreExpr::Unary(op, inner) => XreExpr::Unary(*op, Box::new(strip_groups(inner))),
        XreExpr::Binary(op, l, r) => {
            XreExpr::Binary(*op, Box::new(strip_groups(l)), Box::new(strip_groups(r)))
        }
        XreExpr::RepeatN(e, n) => XreExpr::RepeatN(Box::new(strip_groups(e)), *n),
        XreExpr::RepeatNPlus(e, n) => XreExpr::RepeatNPlus(Box::new(strip_groups(e)), *n),
        XreExpr::RepeatNMinus(e, n) => XreExpr::RepeatNMinus(Box::new(strip_groups(e)), *n),
        XreExpr::RepeatNToK(e, n, k) => XreExpr::RepeatNToK(Box::new(strip_groups(e)), *n, *k),
        XreExpr::ContainmentWithWeight { expr, weight } => XreExpr::ContainmentWithWeight {
            expr: Box::new(strip_groups(expr)),
            weight: *weight,
        },
        XreExpr::Replace { arrow, rules } => XreExpr::Replace {
            arrow: *arrow,
            rules: rules.iter().map(strip_groups_rule).collect(),
        },
        XreExpr::Restriction { body, contexts } => XreExpr::Restriction {
            body: Box::new(strip_groups(body)),
            contexts: contexts
                .iter()
                .map(|c| RestrContext {
                    left: c.left.as_ref().map(|b| Box::new(strip_groups(b))),
                    right: c.right.as_ref().map(|b| Box::new(strip_groups(b))),
                })
                .collect(),
        },
        XreExpr::Substitute { haystack, what } => XreExpr::Substitute {
            haystack: Box::new(strip_groups(haystack)),
            what: what.clone(),
        },
    };
    Spanned::new(value, span)
}

fn strip_groups_rule(rule: &ReplaceRule) -> ReplaceRule {
    ReplaceRule {
        mappings: rule.mappings.iter().map(strip_groups_mapping).collect(),
        contexts: rule.contexts.as_ref().map(|cx| ReplaceContexts {
            mark: cx.mark,
            items: cx
                .items
                .iter()
                .map(|c| ReplaceContext {
                    left: c.left.as_ref().map(|b| Box::new(strip_groups(b))),
                    right: c.right.as_ref().map(|b| Box::new(strip_groups(b))),
                })
                .collect(),
        }),
    }
}

fn strip_groups_mapping(m: &MappingPair) -> MappingPair {
    MappingPair {
        upper: strip_groups_side(&m.upper),
        arrow: m.arrow,
        kind: match &m.kind {
            MappingKind::Plain { lower } => MappingKind::Plain {
                lower: strip_groups_side(lower),
            },
            MappingKind::Markup { pre, post } => MappingKind::Markup {
                pre: pre.as_ref().map(strip_groups_side),
                post: post.as_ref().map(strip_groups_side),
            },
        },
    }
}

fn strip_groups_side(side: &MappingSide) -> MappingSide {
    match side {
        MappingSide::Expr(b) => MappingSide::Expr(Box::new(strip_groups(b))),
        MappingSide::Dotted(opt) => {
            MappingSide::Dotted(opt.as_ref().map(|b| Box::new(strip_groups(b))))
        }
    }
}

// ───────────────────────── writer core ─────────────────────────

fn write_top(out: &mut SmolStrBuilder, expr: &XreExpr) {
    write_expr(out, expr);
}

fn write_expr(out: &mut SmolStrBuilder, expr: &XreExpr) {
    match expr {
        XreExpr::Symbol(s) => out.push_str(&escape_symbol(s)),
        XreExpr::Curly(s) => {
            out.push('{');
            out.push_str(s);
            out.push('}');
        }
        XreExpr::Epsilon => out.push('0'),
        XreExpr::Any => out.push('?'),
        XreExpr::BoundaryMarker => out.push_str(".#."),

        XreExpr::Pair { upper, lower } => {
            write_atom_or_bracketed(out, &upper.value);
            out.push(':');
            write_atom_or_bracketed(out, &lower.value);
        }

        XreExpr::Weighted { expr, weight } => {
            write_atom_or_bracketed(out, &expr.value);
            let _ = write!(out, "::{weight}");
        }

        XreExpr::ReadFile { kind, path } => {
            let prefix = match kind {
                ReadKind::Binary => "@bin",
                ReadKind::Text => "@txt",
                ReadKind::Spaced => "@stxt",
                ReadKind::Prolog => "@pl",
                ReadKind::Regex => "@re",
            };
            let _ = write!(out, "{prefix}\"{path}\"");
        }

        XreExpr::FunctionCall { name, args } => {
            out.push_str(name);
            out.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_expr(out, &arg.value);
            }
            out.push(')');
        }

        XreExpr::Group(inner) => {
            out.push('[');
            write_expr(out, &inner.value);
            out.push(']');
        }

        XreExpr::Optional(inner) => {
            out.push('(');
            write_expr(out, &inner.value);
            out.push(')');
        }

        XreExpr::BracketedDotted(None) => out.push_str("[..]"),
        XreExpr::BracketedDotted(Some(inner)) => {
            out.push_str("[. ");
            write_expr(out, &inner.value);
            out.push_str(" .]");
        }

        XreExpr::Unary(op, inner) => write_unary(out, *op, &inner.value),

        XreExpr::Binary(op, l, r) => write_binary(out, *op, &l.value, &r.value),

        XreExpr::RepeatN(e, n) => {
            write_atom_or_bracketed(out, &e.value);
            let _ = write!(out, "^{n}");
        }
        XreExpr::RepeatNPlus(e, n) => {
            write_atom_or_bracketed(out, &e.value);
            let _ = write!(out, "^>{n}");
        }
        XreExpr::RepeatNMinus(e, n) => {
            write_atom_or_bracketed(out, &e.value);
            let _ = write!(out, "^<{n}");
        }
        XreExpr::RepeatNToK(e, n, k) => {
            write_atom_or_bracketed(out, &e.value);
            let _ = write!(out, "^{n},{k}");
        }

        XreExpr::ContainmentWithWeight { expr, weight } => {
            let _ = write!(out, "$::{weight} ");
            write_atom_or_bracketed(out, &expr.value);
        }

        XreExpr::Replace { rules, .. } => write_replace(out, rules),

        XreExpr::Restriction { body, contexts } => {
            write_atom_or_bracketed(out, &body.value);
            out.push_str(" => ");
            for (i, cx) in contexts.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_restr_context(out, cx);
            }
        }

        XreExpr::Substitute { haystack, what } => {
            out.push_str("`[ ");
            write_expr(out, &haystack.value);
            out.push_str(" , ");
            write_substitute_what(out, what);
            out.push_str(" ]");
        }
    }
}

/// Wrap any compound expression in `[...]`. Atoms pass through unchanged.
fn write_atom_or_bracketed(out: &mut SmolStrBuilder, expr: &XreExpr) {
    if is_atomic(expr) {
        write_expr(out, expr);
    } else {
        out.push('[');
        write_expr(out, expr);
        out.push(']');
    }
}

fn is_atomic(expr: &XreExpr) -> bool {
    matches!(
        expr,
        XreExpr::Symbol(_)
            | XreExpr::Curly(_)
            | XreExpr::Epsilon
            | XreExpr::Any
            | XreExpr::BoundaryMarker
            | XreExpr::Group(_)
            | XreExpr::Optional(_)
            | XreExpr::BracketedDotted(_)
            | XreExpr::ReadFile { .. }
            | XreExpr::FunctionCall { .. }
    )
}

fn write_unary(out: &mut SmolStrBuilder, op: UnaryOp, inner: &XreExpr) {
    match op {
        // postfix
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
        // prefix
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

fn write_binary(out: &mut SmolStrBuilder, op: BinaryOp, l: &XreExpr, r: &XreExpr) {
    let sep = match op {
        BinaryOp::Concatenate => " ",
        BinaryOp::Compose => " .o. ",
        BinaryOp::LenientCompose => " .O. ",
        BinaryOp::CrossProduct => " .x. ",
        BinaryOp::MergeRight => " .m>. ",
        BinaryOp::MergeLeft => " .<m. ",
        BinaryOp::Before => " < ",
        BinaryOp::After => " > ",
        BinaryOp::Shuffle => " <> ",
        BinaryOp::Union => " | ",
        BinaryOp::Intersect => " & ",
        BinaryOp::Subtract => " - ",
        BinaryOp::UpperSubtract => " .-u. ",
        BinaryOp::LowerSubtract => " .-l. ",
        BinaryOp::UpperPriorityUnion => " .P. ",
        BinaryOp::LowerPriorityUnion => " .p. ",
        BinaryOp::Ignoring => " / ",
        BinaryOp::IgnoreInternally => " ./. ",
        BinaryOp::LeftQuotient => " \\\\\\ ",
    };
    write_atom_or_bracketed(out, l);
    out.push_str(sep);
    write_atom_or_bracketed(out, r);
}

fn write_replace(out: &mut SmolStrBuilder, rules: &[ReplaceRule]) {
    for (i, rule) in rules.iter().enumerate() {
        if i > 0 {
            out.push_str(" ,, ");
        }
        for (j, m) in rule.mappings.iter().enumerate() {
            if j > 0 {
                out.push_str(" , ");
            }
            write_mapping_pair(out, m, replace_arrow_str(m.arrow));
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
    out.push(' ');
    out.push_str(arrow_str);
    out.push(' ');
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

fn write_substitute_what(out: &mut SmolStrBuilder, what: &SubstituteWhat) {
    match what {
        SubstituteWhat::Symbol {
            needle,
            replacement,
        } => {
            out.push_str(&escape_symbol(needle));
            out.push_str(" , ");
            for (i, sym) in replacement.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push_str(&escape_symbol(sym));
            }
        }
        SubstituteWhat::Pair { from, to } => {
            out.push_str(&escape_symbol(&from.0));
            out.push(':');
            out.push_str(&escape_symbol(&from.1));
            out.push_str(" , ");
            out.push_str(&escape_symbol(&to.0));
            out.push(':');
            out.push_str(&escape_symbol(&to.1));
        }
    }
}

fn replace_arrow_str(a: ReplaceArrow) -> &'static str {
    match a {
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
    }
}

fn context_mark_str(m: ContextMark) -> &'static str {
    match m {
        ContextMark::UpperUpper => "||",
        ContextMark::LowerUpper => "//",
        ContextMark::UpperLower => "\\\\",
        ContextMark::LowerLower => "\\/",
    }
}

/// Insert `%` before any character that is special in xre's lexer. Mirrors
/// the inverse of `strip_percents`.
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
            | '!'
            | ','
            | '.'
            | '^'
            | ':'
            | '"'
            | ';'
            | '@'
            | '0'
            | '~'
            | '\\'
            | '&'
            | '?'
            | '$'
            | '+'
            | '*'
            | '/'
            | '_'
            | '('
            | ')'
            | '{'
            | '}'
            | ']'
            | '['
            | '#'
            | '`'
            | '\''
            | '='
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    /// Round-trip: parse → pretty → re-parse → strip_groups on both, compare.
    fn round_trip(src: &str) {
        let tree = parse(src).unwrap_or_else(|e| panic!("first parse of {src:?} failed: {e:?}"));
        let printed = pretty_print(&tree);
        let re_parsed = parse(&printed).unwrap_or_else(|e| {
            panic!("re-parse of pretty-printed {src:?}\n  printed: {printed}\n  err: {e:?}")
        });
        let lhs = strip_groups(&tree);
        let rhs = strip_groups(&re_parsed);
        assert_eq!(
            lhs.value, rhs.value,
            "round-trip differed for {src:?}\n  printed: {printed}\n  lhs: {:#?}\n  rhs: {:#?}",
            lhs.value, rhs.value
        );
    }

    #[test]
    fn rt_atoms() {
        round_trip("a");
        round_trip("0");
        round_trip(r#""""#);
        round_trip("?");
        round_trip(".#.");
        round_trip("{abc}");
    }

    #[test]
    fn rt_pair_and_weight() {
        round_trip("a:b");
        round_trip("a::1.5");
        round_trip("a:{xy}");
        round_trip("{xy}:a");
    }

    #[test]
    fn rt_unary_postfix() {
        round_trip("a*");
        round_trip("a+");
        round_trip("a.r");
        round_trip("a.i");
        round_trip("a.u");
        round_trip("a.l");
    }

    #[test]
    fn rt_unary_prefix() {
        round_trip("~a");
        round_trip("\\a");
        round_trip("$a");
        round_trip("$.a");
        round_trip("$?a");
    }

    #[test]
    fn rt_repeat_n() {
        round_trip("a^3");
        round_trip("a^>3");
        round_trip("a^<3");
        round_trip("a^3,5");
    }

    #[test]
    fn rt_concat_and_union() {
        round_trip("a b c");
        round_trip("a | b");
        round_trip("a b | c");
        round_trip("a b c | d e f");
    }

    #[test]
    fn rt_composition_family() {
        round_trip("a .o. b");
        round_trip("a .O. b");
        round_trip("a .x. b");
        round_trip("a .m>. b");
        round_trip("a .<m. b");
    }

    #[test]
    fn rt_brackets() {
        round_trip("[a]");
        round_trip("(a)");
        round_trip("[..]");
        round_trip("[. a .]");
    }

    #[test]
    fn rt_complement_containment() {
        round_trip("~$[ a ]");
        round_trip("~ $ a b c");
    }

    #[test]
    fn rt_replace_simple() {
        round_trip("a -> b");
        round_trip("0 <- a");
        round_trip("a -> b , c -> d");
        round_trip("a -> b ,, c -> d");
    }

    #[test]
    fn rt_replace_with_contexts() {
        round_trip("a -> b || c _ d");
        round_trip("a -> b // c _");
    }

    #[test]
    fn rt_replace_markup() {
        round_trip("a -> b ... c");
        round_trip("a -> b ...");
        round_trip("a -> ... c");
    }

    #[test]
    fn rt_replace_dotted() {
        round_trip("[..] -> a");
        round_trip("a -> [. b .]");
    }

    #[test]
    fn rt_restriction() {
        round_trip("a => b _ c");
    }

    #[test]
    fn rt_substitute() {
        round_trip("`[ a , b , c d ]");
        round_trip("`[ a , b:c , d:e ]");
    }

    #[test]
    fn rt_function_call() {
        round_trip("Foo()");
        round_trip("Concat(a, b)");
    }

    #[test]
    fn rt_read_file() {
        round_trip(r#"@bin"x.bin""#);
        round_trip(r#"@txt"x.txt""#);
        round_trip(r#"@"x.fst""#);
    }

    #[test]
    fn rt_escapes_special_chars_in_symbol() {
        // %+N stripped to "+N"; on emit, "+" is escaped back.
        round_trip("%+N");
    }

    #[test]
    fn rt_corpus_fixtures() {
        let fixtures_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut paths: Vec<_> = std::fs::read_dir(&fixtures_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "xre").unwrap_or(false))
            .map(|e| e.path())
            .collect();
        paths.sort();
        for path in paths {
            let src = std::fs::read_to_string(&path).unwrap();
            // Use parse_all because some fixtures are multi-expression.
            let trees = match crate::parse_all(&src) {
                Ok(ts) => ts,
                Err(_) => continue, // skip files that don't parse with parse_all (none expected)
            };
            for (i, tree) in trees.iter().enumerate() {
                let printed = pretty_print(tree);
                let re = match parse(&printed) {
                    Ok(t) => t,
                    Err(e) => panic!(
                        "fixture {} expr#{} re-parse failed:\n  printed: {printed}\n  err: {:?}",
                        path.display(),
                        i,
                        e
                    ),
                };
                let lhs = strip_groups(tree);
                let rhs = strip_groups(&re);
                assert_eq!(
                    lhs.value,
                    rhs.value,
                    "fixture {} expr#{} round-trip diverged\n  printed: {printed}",
                    path.display(),
                    i
                );
            }
        }
    }
}
