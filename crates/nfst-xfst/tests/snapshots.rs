//! Per-AST-variant snapshot tests for nfst-xfst.

use nfst_xfst::{
    ApplyKind, NetworkOp, PrintCmd, ReadCmd, RedirectKind, SaveCmd, SubstituteCmd, TestKind,
    XfstCommand, parse, pretty_print,
};

fn parsed(src: &str) -> Vec<XfstCommand> {
    let f = parse(src).unwrap_or_else(|e| panic!("parse {src:?}: {e:?}"));
    f.value.commands.into_iter().map(|c| c.value).collect()
}

/// Pretty-printed form, for asserting that two spellings of the same script
/// mean the same thing without comparing spans.
fn printed(src: &str) -> String {
    let f = parse(src).unwrap_or_else(|e| panic!("parse {src:?}: {e:?}"));
    pretty_print(&f)
}

// ───────────────── trivial / structural ─────────────────

#[test]
fn empty_script_has_no_commands() {
    let cmds = parsed("");
    assert!(cmds.is_empty());
}

#[test]
fn comment_lines_are_skipped() {
    let cmds = parsed("! comment\nclear ;\n# another\npop ;");
    assert_eq!(cmds.len(), 2);
    assert!(matches!(cmds[0], XfstCommand::Clear));
    assert!(matches!(cmds[1], XfstCommand::Pop));
}

#[test]
fn semicolons_are_optional_separators() {
    let cmds = parsed("clear\npop\nquit");
    assert_eq!(cmds.len(), 3);
}

// ───────────────── regex / define ─────────────────

#[test]
fn regex_command_embeds_xre() {
    let cmds = parsed("regex a:b ;");
    assert!(matches!(cmds[0], XfstCommand::Regex(_)));
}

// Regression: divvun/hfst-rs#2. Upstream enters an exclusive lexer state on
// `regex` and that state spans newlines, so the body need not start on the
// same line. We used to treat the newline as an empty body and then dispatch
// the body's first token as a command — `unknown command '['`.
#[test]
fn regex_body_may_start_on_the_next_line() {
    assert_eq!(
        printed("regex\n[ Y | y ] (->) [ y e s ] ;\nsave stack yes.hfst\n"),
        printed("regex [ Y | y ] (->) [ y e s ] ;\nsave stack yes.hfst\n"),
    );
}

#[test]
fn read_regex_body_may_start_on_the_next_line() {
    assert_eq!(
        printed("read regex\n[ a | b ] ;"),
        printed("regex [ a | b ] ;")
    );
}

#[test]
fn regex_body_may_be_preceded_by_blank_and_comment_lines() {
    // The comment stays in the body and the regex parser strips it.
    assert_eq!(
        printed("regex\n\n! pick a letter\n[ a | b ] ;"),
        printed("regex [ a | b ] ;"),
    );
}

#[test]
fn define_function_body_may_start_on_the_next_line() {
    // A prototype enters the same state upstream (xfst-lexer.ll:239).
    let cmds = parsed("define Concat(x, y)\nx y ;");
    if let XfstCommand::DefineFunction { name, params, .. } = &cmds[0] {
        assert_eq!(name, "Concat");
        assert_eq!(params, &vec!["x".to_string(), "y".to_string()]);
    } else {
        panic!("expected DefineFunction, got {:?}", cmds[0]);
    }
}

// The counterpart the fix must NOT break: a bare `define NAME` at end of line
// is upstream's empty declaration form (xfst-lexer.ll:234), defining from the
// top of the stack. For it, a newline still ends the body.
#[test]
fn bare_define_at_end_of_line_does_not_swallow_the_next_command() {
    let cmds = parsed("define Foo\nregex [ a ] ;");
    assert_eq!(cmds.len(), 2);
    assert!(
        matches!(&cmds[0], XfstCommand::Define { name, .. } if name == "Foo"),
        "expected a bodyless Define, got {:?}",
        cmds[0]
    );
    assert!(matches!(cmds[1], XfstCommand::Regex(_)));
}

// A `;` inside a `!` comment is invisible to the regex parser upstream, so it
// must not terminate the body here either.
#[test]
fn semicolon_inside_a_comment_does_not_end_the_regex() {
    assert_eq!(
        printed("regex a ! stop ; here\n| b ;"),
        printed("regex a | b ;"),
    );
}

// Why the comment handling above is `!`-only: `#` also opens a comment, but
// only in token position — mid-token it is an ordinary symbol character.
#[test]
fn hash_inside_a_multichar_symbol_is_not_a_comment() {
    // If `#` were treated as a comment start, it would swallow the `;` and
    // the `pop` would be absorbed into the regex body.
    let cmds = parsed("regex abc#def ;\npop ;");
    assert_eq!(cmds.len(), 2);
    assert!(matches!(cmds[0], XfstCommand::Regex(_)));
    assert!(matches!(cmds[1], XfstCommand::Pop));
}

#[test]
fn define_records_name_and_body() {
    let cmds = parsed("define Foo a b ;");
    if let XfstCommand::Define { name, .. } = &cmds[0] {
        assert_eq!(name, "Foo");
    } else {
        panic!("expected Define");
    }
}

// A prototype's `(` must be ADJACENT to the name — C++ lexes the whole thing
// as a single token, `{NAMETOKEN}"("...`. With a space between them it is a
// plain net definition whose body merely opens with a parenthesised (that is,
// optional) expression.
//
// Treating `define A (r) g -> [k k] ;` as a function yielded an empty
// transducer at every reference. In Giella lang-kal such a rule sat in a
// ~50-rule `.o.` chain, so one empty member annihilated the composition and
// the language's analyser came out with 1 state and 0 arcs — while the build
// still reported success.
#[test]
fn parenthesised_optional_after_a_space_is_a_net_not_a_function() {
    let cmds = parsed("define A (r) g -> [ k k ] ;");
    assert!(
        matches!(&cmds[0], XfstCommand::Define { name, .. } if name == "A"),
        "expected a net Define, got {:?}",
        cmds[0]
    );
}

#[test]
fn prototype_adjacent_to_the_name_is_still_a_function() {
    let cmds = parsed("define A(r) g -> [ k k ] ;");
    if let XfstCommand::DefineFunction { name, params, .. } = &cmds[0] {
        assert_eq!(name, "A");
        assert_eq!(params, &vec!["r".to_string()]);
    } else {
        panic!("expected DefineFunction, got {:?}", cmds[0]);
    }
}

#[test]
fn define_function_records_params() {
    let cmds = parsed("define Concat(x, y) x y ;");
    if let XfstCommand::DefineFunction { name, params, .. } = &cmds[0] {
        assert_eq!(name, "Concat");
        assert_eq!(params, &vec!["x".to_string(), "y".to_string()]);
    } else {
        panic!("expected DefineFunction, got {:?}", cmds[0]);
    }
}

#[test]
fn define_with_no_body_is_declaration() {
    let cmds = parsed("define Foo ;");
    assert!(matches!(cmds[0], XfstCommand::Define { .. }));
}

#[test]
fn define_alias_records_body() {
    let cmds = parsed("alias clearall clear pop ;");
    if let XfstCommand::DefineAlias { name, body } = &cmds[0] {
        assert_eq!(name, "clearall");
        assert!(body.contains("clear"));
        assert!(body.contains("pop"));
    } else {
        panic!("expected DefineAlias");
    }
}

#[test]
fn undefine_takes_name_list() {
    let cmds = parsed("undefine Foo Bar Baz ;");
    if let XfstCommand::Undefine(names) = &cmds[0] {
        assert_eq!(
            names,
            &vec!["Foo".to_string(), "Bar".to_string(), "Baz".to_string()]
        );
    } else {
        panic!("expected Undefine");
    }
}

#[test]
fn list_records_members() {
    let cmds = parsed("list Vowels a e i o u ;");
    if let XfstCommand::DefineList { name, members } = &cmds[0] {
        assert_eq!(name, "Vowels");
        assert_eq!(members.len(), 5);
    } else {
        panic!("expected DefineList, got {:?}", cmds[0]);
    }
}

// ───────────────── stack ─────────────────

#[test]
fn clear_pop_turn_rotate() {
    let cmds = parsed("clear ;\npop ;\nturn ;\nrotate ;");
    assert!(matches!(cmds[0], XfstCommand::Clear));
    assert!(matches!(cmds[1], XfstCommand::Pop));
    assert!(matches!(cmds[2], XfstCommand::Turn));
    assert!(matches!(cmds[3], XfstCommand::Rotate));
}

#[test]
fn push_records_target() {
    let cmds = parsed("push Foo");
    if let XfstCommand::Push(t) = &cmds[0] {
        assert_eq!(t, "Foo");
    } else {
        panic!("expected Push");
    }
}

#[test]
fn load_stack_records_path() {
    let cmds = parsed("load stack myfst.hfst ;");
    if let XfstCommand::LoadStack(p) = &cmds[0] {
        assert_eq!(p, "myfst.hfst");
    } else {
        panic!("expected LoadStack");
    }
}

// ───────────────── network ops ─────────────────

#[test]
fn compose_two_word_form() {
    assert!(matches!(
        parsed("compose net ;")[0],
        XfstCommand::Network(NetworkOp::Compose)
    ));
}

#[test]
fn compose_one_word_form() {
    assert!(matches!(
        parsed("compose ;")[0],
        XfstCommand::Network(NetworkOp::Compose)
    ));
}

#[test]
fn binary_ops_collapse_aliases() {
    // intersect / conjunct → Intersect; union / disjunct → Union.
    assert!(matches!(
        parsed("conjunct ;")[0],
        XfstCommand::Network(NetworkOp::Intersect)
    ));
    assert!(matches!(
        parsed("disjunct ;")[0],
        XfstCommand::Network(NetworkOp::Union)
    ));
    assert!(matches!(
        parsed("subtract ;")[0],
        XfstCommand::Network(NetworkOp::Minus)
    ));
}

#[test]
fn unary_ops_recognised() {
    let cmds = parsed("invert ;\nminimize ;\ndeterminize ;\nreverse ;");
    assert!(matches!(cmds[0], XfstCommand::Network(NetworkOp::Invert)));
    assert!(matches!(cmds[1], XfstCommand::Network(NetworkOp::Minimize)));
    assert!(matches!(
        cmds[2],
        XfstCommand::Network(NetworkOp::Determinize)
    ));
    assert!(matches!(cmds[3], XfstCommand::Network(NetworkOp::Reverse)));
}

#[test]
fn eliminate_flag_takes_argument() {
    let cmds = parsed("eliminate flag F ;");
    if let XfstCommand::Network(NetworkOp::EliminateFlag(name)) = &cmds[0] {
        assert_eq!(name, "F");
    } else {
        panic!("expected EliminateFlag, got {:?}", cmds[0]);
    }
}

// ───────────────── apply ─────────────────

#[test]
fn apply_up_inline_form() {
    let cmds = parsed("apply up cat\n");
    if let XfstCommand::Apply(ApplyKind::Up, body) = &cmds[0] {
        assert_eq!(body.as_deref(), Some("cat"));
    } else {
        panic!("expected Apply Up, got {:?}", cmds[0]);
    }
}

#[test]
fn apply_down_inline_form() {
    let cmds = parsed("apply down cat\n");
    assert!(matches!(
        cmds[0],
        XfstCommand::Apply(ApplyKind::Down, Some(_))
    ));
}

#[test]
fn apply_up_heredoc_form() {
    let cmds = parsed("apply up\nfoo\nbar\n<ctrl-d>\nquit");
    if let XfstCommand::Apply(ApplyKind::Up, Some(body)) = &cmds[0] {
        assert!(body.contains("foo"));
        assert!(body.contains("bar"));
    } else {
        panic!("expected heredoc Apply Up");
    }
    assert!(matches!(cmds[1], XfstCommand::Quit));
}

#[test]
fn apply_med_command() {
    let cmds = parsed("apply med\n<ctrl-d>\nquit");
    assert!(matches!(cmds[0], XfstCommand::Apply(ApplyKind::Med, _)));
}

// ───────────────── read / save ─────────────────

#[test]
fn read_lexc_records_path() {
    let cmds = parsed("read lexc input.lexc ;");
    assert!(matches!(cmds[0], XfstCommand::Read(ReadCmd::Lexc(_))));
}

#[test]
fn read_att_records_path() {
    let cmds = parsed("read att input.att ;");
    assert!(matches!(cmds[0], XfstCommand::Read(ReadCmd::Att(_))));
}

#[test]
fn read_text_heredoc_form() {
    let cmds = parsed("read text\nfoo\nbar\n<ctrl-d>\n");
    if let XfstCommand::Read(ReadCmd::Text(b)) = &cmds[0] {
        assert!(b.contains("foo"));
    } else {
        panic!("expected Read Text");
    }
}

#[test]
fn save_stack_records_path() {
    let cmds = parsed("save stack out.hfst ;");
    if let XfstCommand::Save(SaveCmd::Stack(p)) = &cmds[0] {
        assert_eq!(p, "out.hfst");
    } else {
        panic!("expected Save Stack");
    }
}

#[test]
fn write_att_form() {
    let cmds = parsed("write att out.att ;");
    assert!(matches!(cmds[0], XfstCommand::Save(SaveCmd::Att(_))));
}

// ───────────────── print ─────────────────

#[test]
fn print_net_default_form() {
    assert!(matches!(
        parsed("print net ;")[0],
        XfstCommand::Print(PrintCmd::Net)
    ));
}

#[test]
fn print_words_with_count() {
    let cmds = parsed("print words 5 ;");
    if let XfstCommand::Print(PrintCmd::Words(n)) = &cmds[0] {
        assert_eq!(*n, Some(5));
    } else {
        panic!("expected Print Words(5)");
    }
}

#[test]
fn print_words_default_no_count() {
    if let XfstCommand::Print(PrintCmd::Words(n)) = &parsed("print words ;")[0] {
        assert_eq!(*n, None);
    }
}

#[test]
fn print_labels_with_optional_arg() {
    let cmds = parsed("print labels Foo");
    if let XfstCommand::Print(PrintCmd::Labels(arg)) = &cmds[0] {
        assert_eq!(arg.as_deref(), Some("Foo"));
    } else {
        panic!("expected Print Labels");
    }
}

#[test]
fn print_labels_no_arg() {
    let cmds = parsed("print labels");
    if let XfstCommand::Print(PrintCmd::Labels(arg)) = &cmds[0] {
        assert!(arg.is_none());
    }
}

#[test]
fn print_random_words_count() {
    if let XfstCommand::Print(PrintCmd::RandomWords(n)) = &parsed("print random-words 10 ;")[0] {
        assert_eq!(*n, Some(10));
    }
}

// ───────────────── test ─────────────────

#[test]
fn every_test_kind_recognised() {
    let pairs: &[(&str, TestKind)] = &[
        ("test equivalent ;", TestKind::Eq),
        ("test functional ;", TestKind::Funct),
        ("test identity ;", TestKind::Id),
        ("test null ;", TestKind::Null),
        ("test non-null ;", TestKind::Nonnull),
        ("test overlap ;", TestKind::Overlap),
        ("test sublanguage ;", TestKind::Sublanguage),
        ("test unambiguous ;", TestKind::Unambiguous),
        ("test infinitely-ambiguous ;", TestKind::InfinitelyAmbiguous),
        ("test lower-bounded ;", TestKind::LowerBounded),
        ("test lower-universal ;", TestKind::LowerUni),
        ("test upper-bounded ;", TestKind::UpperBounded),
        ("test upper-universal ;", TestKind::UpperUni),
    ];
    for (src, expected) in pairs {
        let cmds = parsed(src);
        if let XfstCommand::Test(k) = &cmds[0] {
            assert_eq!(k, expected, "input: {src}");
        } else {
            panic!("expected Test for {src}");
        }
    }
}

// ───────────────── shell / system ─────────────────

#[test]
fn quit_alone() {
    assert!(matches!(parsed("quit")[0], XfstCommand::Quit));
}

#[test]
fn quit_synonyms_collapse() {
    for kw in ["quit", "exit", "bye", "stop", "has"] {
        assert!(matches!(parsed(kw)[0], XfstCommand::Quit), "{kw}");
    }
}

#[test]
fn echo_takes_rest_of_line() {
    if let XfstCommand::Echo(t) = &parsed("echo hello world\n")[0] {
        assert_eq!(t, "hello world");
    } else {
        panic!("expected Echo");
    }
}

#[test]
fn system_takes_rest_of_line() {
    if let XfstCommand::System(t) = &parsed("system ls -l\n")[0] {
        assert_eq!(t, "ls -l");
    } else {
        panic!("expected System");
    }
}

#[test]
fn source_records_path() {
    if let XfstCommand::Source(p) = &parsed("source extra.xfst ;")[0] {
        assert_eq!(p, "extra.xfst");
    } else {
        panic!("expected Source");
    }
}

// ───────────────── variables ─────────────────

#[test]
fn set_records_var_value() {
    let cmds = parsed("set quit-on-fail ON\n");
    if let XfstCommand::Set { var, value } = &cmds[0] {
        assert_eq!(var, "quit-on-fail");
        assert_eq!(value, "ON");
    } else {
        panic!("expected Set");
    }
}

#[test]
fn show_variables_form() {
    let cmds = parsed("show variables ;");
    if let XfstCommand::Show(v) = &cmds[0] {
        assert!(v.is_none());
    } else {
        panic!("expected Show(None)");
    }
}

#[test]
fn show_specific_variable() {
    let cmds = parsed("show quit-on-fail ;");
    if let XfstCommand::Show(Some(v)) = &cmds[0] {
        assert_eq!(v, "quit-on-fail");
    } else {
        panic!("expected Show(Some)");
    }
}

// ───────────────── substitute ─────────────────

#[test]
fn substitute_symbol_simple() {
    let cmds = parsed("substitute symbol A for a\n");
    if let XfstCommand::Substitute(SubstituteCmd::Symbol { from, to, .. }) = &cmds[0] {
        assert_eq!(from, &vec!["A".to_string()]);
        assert_eq!(to, "a");
    } else {
        panic!("expected Substitute Symbol, got {:?}", cmds[0]);
    }
}

#[test]
fn substitute_symbol_multiple_from() {
    let cmds = parsed("substitute symbol A B C for a\n");
    if let XfstCommand::Substitute(SubstituteCmd::Symbol { from, .. }) = &cmds[0] {
        assert_eq!(from.len(), 3);
    } else {
        panic!("expected multi-from Symbol substitute");
    }
}

#[test]
fn substitute_label_with_pair() {
    let cmds = parsed("substitute label A:B for a:b\n");
    if let XfstCommand::Substitute(SubstituteCmd::Label { from, to, .. }) = &cmds[0] {
        assert_eq!(from, &vec!["A:B".to_string()]);
        assert_eq!(to, "a:b");
    } else {
        panic!("expected Substitute Label");
    }
}

#[test]
fn substitute_defined_form() {
    let cmds = parsed("substitute defined Foo for x\n");
    if let XfstCommand::Substitute(SubstituteCmd::Named { def, label }) = &cmds[0] {
        assert_eq!(def, "Foo");
        assert_eq!(label, "x");
    } else {
        panic!("expected Substitute Named");
    }
}

// ───────────────── redirects ─────────────────

#[test]
fn print_with_out_redirect() {
    let cmds = parsed("print net > out.txt ;");
    if let XfstCommand::Redirected { redirect, .. } = &cmds[0] {
        assert_eq!(redirect.kind, RedirectKind::Out);
        assert_eq!(redirect.path, "out.txt");
    } else {
        panic!("expected Redirected");
    }
}

#[test]
fn append_redirect() {
    let cmds = parsed("print net >> log.txt ;");
    if let XfstCommand::Redirected { redirect, .. } = &cmds[0] {
        assert_eq!(redirect.kind, RedirectKind::Append);
    } else {
        panic!("expected append Redirected");
    }
}

#[test]
fn input_redirect() {
    let cmds = parsed("load < in.hfst ;");
    if let XfstCommand::Redirected { redirect, .. } = &cmds[0] {
        assert_eq!(redirect.kind, RedirectKind::In);
    } else {
        panic!("expected input Redirected");
    }
}

// ───────────────── assert prefix ─────────────────

#[test]
fn assert_wraps_inner_command() {
    let cmds = parsed("assert test null ;");
    if let XfstCommand::Assert(inner) = &cmds[0] {
        assert!(matches!(inner.value, XfstCommand::Test(TestKind::Null)));
    } else {
        panic!("expected Assert");
    }
}

// ───────────────── span sanity ─────────────────

#[test]
fn span_on_top_script() {
    let f = parse("clear ;\nquit").unwrap();
    assert!(f.span.start() < f.span.end());
}
