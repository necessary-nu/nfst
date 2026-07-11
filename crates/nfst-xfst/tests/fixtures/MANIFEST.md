# xfst fixture corpus

## Real-world (~189 files)

Vendored from `~/git/divvun/hfst-rs/lib/`. Filenames encode the source
path so files with the same basename across repos don't collide:

- `parsers__test__*.xfst` — the bulk of the corpus, focused single-
  command parser tests covering every replace variant, regex shape,
  apply call, compose/intersect, substitute, define, write-att, etc.
- `scripts__*.xfst` — integration-style multi-command scripts.
- `python__*.xfst` — the small Python-test fixtures.

The four upstream test files that are *intentionally invalid* for
parse-only consumption are excluded:

- `inspect_net.xfst` — `inspect net` is interactive; subsequent lines
  are stdin, not xfst commands.
- `quit-on-fail.xfst` — contains `regex [;` (deliberately malformed).
- `replace_test_flags_2.xfst` — contains a malformed regex body.
- `test_fail.xfst` — contains the typo `pritn` (intentional failure).

## Curated single-construct snippets

Hand-written to give each AST variant clean, isolated coverage:

- `snippet-regex-and-define.xfst`   — `regex` + `define NAME E ;`
- `snippet-define-function.xfst`    — `define NAME(args) E ;` form
- `snippet-stack-ops.xfst`          — `clear`, `pop`, `turn`, `rotate`
- `snippet-network-ops.xfst`        — `compose`, `invert`, `minimize`,
                                      `determinize`, `reverse`
- `snippet-apply-heredoc.xfst`      — `apply up <body> <ctrl-d>`
- `snippet-apply-inline.xfst`       — `apply up X` single-line form
- `snippet-redirects.xfst`          — `> path` / `< path` redirection
- `snippet-substitute.xfst`         — all three substitute kinds
- `snippet-test-suite.xfst`         — every `test` variant
- `snippet-print-family.xfst`       — `print` variants with counts
- `snippet-set-show.xfst`           — `set` / `show variables`
- `snippet-assert-prefix.xfst`      — `assert <command>` wrapper
