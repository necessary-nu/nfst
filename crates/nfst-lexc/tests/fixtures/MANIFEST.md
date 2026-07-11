# lexc fixture corpus

Vendored from `/tmp/hfst-codex/test/tools/*.lexc` (HFST `lexc-compiler`
test suite). Each file is a small lexc source. The annotations below were
extracted from `lexc-compiler-functionality.sh` and map fixture
behaviours to the upstream `[spec:hfst:sem:lexc.*]` rules.

## Fixture roster

53 `.lexc` source files, organised by feature area:

### Basic surface (28 fixtures)
- `basic.almost-reserved-words.lexc` — entries spelled like keywords
- `basic.cat-dog-bird.lexc` — three trivial entries terminating at `#`
- `basic.colons.lexc` — colon-form entries with various empty sides
- `basic.comments.lexc` — `!` line comments
- `basic.empty-sides.lexc` — `:` (both sides empty)
- `basic.end.lexc` — explicit `END` marker
- `basic.escapes.lexc` — `%`-escaped specials
- `basic.infostrings.lexc` — `"…"` glosses on entries
- `basic.initial-lexicon-empty.lexc` — first lexicon has no entries
- `basic.lowercase-lexicon-end.lexc` — `Lexicon` (titlecase) and `End`
- `basic.multi-entry-lines.lexc` — multiple entries per line
- `basic.multi-file-1.lexc` / `…-2.lexc` / `…-3.lexc` — split lexicons
- `basic.multichar-escaped-zero.lexc` — `%0` in multichar names
- `basic.multichar-flag-with-zero.lexc` — flag-diacritic with `0`
- `basic.multichar-symbol-with-0.lexc` — multichar containing `0`
- `basic.multichar-symbols.lexc` — Multichar_Symbols section
- `basic.no-newline-at-end.lexc` — file ends mid-line
- `basic.no-Root.lexc` — first lexicon is not `Root`
- `basic.punctuation.lexc` — entries containing punctuation
- `basic.regexps.lexc` — `<…>` xre-embedded entries
- `basic.root-loop.lexc` — `Root` references itself
- `basic.spurious-lexicon.lexc` — unused lexicon definition
- `basic.string-pairs.lexc` — `upper:lower` pair forms
- `basic.two-lexicons.lexc` — two-lexicon flow
- `basic.UTF-8.lexc` — non-ASCII codepoints in symbols
- `basic.zeros-epsilons.lexc` — `0` as epsilon shorthand

### XRE-embedded (3 fixtures, exercise the nfst-xre boundary)
- `xre.automatic-multichar-symbols.lexc`
- `xre.definitions.lexc`
- `xre.nested-definitions.lexc`

### Weights & misc (1 fixture)
- `hfst.weights.lexc` — `"weight: N"` glosses (parser stores these as
  glosses; semantics is for the evaluator)

### Tokenisation regressions
- `cat.lexc` — minimal one-entry sanity
- `no-newline-before-sublexicon.lexc` — sublexicon header without newline
- `stress.random-lexicons-100.lexc` — 100 generated lexicons, parser
  stress test
- `tokenize-backtrack.lexc` / `tokenize-dog-in.lexc` — drive the
  multichar tokenizer

### Failure mode (parsed deliberately to exercise diagnostics)
- `test_lexc_fail.lexc` — known to fail compilation in upstream

## Spec rules covered

These IDs come verbatim from `lexc-compiler-functionality.sh` and represent
the requirements the upstream test suite asserts:

- `lexc.compat.continuation-recording.14`
- `lexc.compat.embedded-xre-entry-compilation.12`
- `lexc.compat.embedded-xre-failure-diagnostic.13`
- `lexc.compat.empty-side-epsilon-shorthand.9`
- `lexc.compat.escaping-of-percent-and-angle-brackets.17`
- `lexc.compat.gloss-skipping.10`
- `lexc.compat.lexicon-creation-and-repeated-lexicon-warnings.5`
- `lexc.compat.multicharacter-tokenizer-registration.1`
- `lexc.compat.noflags-storage.3`
- `lexc.compat.one-sided-string-entry-encoding.7`
- `lexc.compat.percent-literal-preservation.16`
- `lexc.compat.percent0-to-atzeroat-conversion.24`
- `lexc.compat.private-definition-marker-encoding.21`
- `lexc.compat.private-joiner-marker-encoding.18`
- `lexc.compat.private-positive-flag-marker-encoding.19`
- `lexc.compat.private-regex-marker-encoding.22`
- `lexc.compat.private-require-flag-marker-encoding.20`
- `lexc.compat.root-lexicon-selection.15`
- `lexc.compat.special-zero-alias-handling.2`
- `lexc.compat.strtod-compatible-weight-parsing.11`
- `lexc.compat.titlecase-lexicon-warning-path.6`
- `lexc.compat.token-position-diagnostics.23`
- `lexc.compat.two-sided-string-pair-alignment.8`
- `lexc.compat.unescaped-0-to-at0at-conversion.25`
- `lexc.compat.unnecessary-escape-warning-whitelist.26`
- `lexc.compat.xre-definition-compilation.4`

Most of these are evaluator-side concerns (alpha-conversion, flag
diacritic generation, weight parsing) and out of scope for the parse-only
port. The ones the parser must satisfy are the `compat.*-encoding` and
`compat.*-recording` rules.
