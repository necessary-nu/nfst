# xre fixture corpus

Vendored from `/tmp/hfst-codex/test/tools/*.xre` (HFST `regexp2fst` test
suite). Each file is a small xre source. The annotations below were
extracted from `regexp2fst-functionality.sh` and map a fixture (or
behaviour) to the `[spec:hfst:sem:tests.xre.*]` rule it exercises.

| Fixture | Spec rule(s) |
|---|---|
| `at_file_quote.foma.xre` | `tests.xre.at-file-quote` |
| `at_file_quote.openfst-tropical.xre` | `tests.xre.at-file-quote` |
| `at_file_quote.sfst.xre` | `tests.xre.at-file-quote` |
| `cats_and_dogs.xre` | `tests.xre.newline-input-cats-dogs`, `tests.xre.info-states-arcs` |
| `cats_and_dogs_semicolon.xre` | `tests.xre.space-separated-semicolon` |
| `left-arrow-with-semicolon-comment.xre` | `tests.xre.left-arrow-comments` |
| `left-arrow-with-semicolon-many-comments.xre` | `tests.xre.left-arrow-comments` |
| `not-contains-a.xre` | `tests.xre.not-contains-a` |
| `not-contains-a-comment-emptyline.xre` | `tests.xre.comment-emptyline` |
| `parallel-left-arrow.xre` | `tests.xre.parallel-left-arrow` |
| `parallel-left-arrow-multicom-emptyline.xre` | `tests.xre.parallel-left-arrow` |

Behavioural rules with no dedicated input file (covered by inline test
strings in the upstream shell script):

- `tests.xre.boundary-symbol-accepted`
- `tests.xre.comment-only-fails`
- `tests.xre.empty-input-fails`
- `tests.xre.freely-insert-ignore`
- `tests.xre.parallel-incompatible-replace-fails`
- `tests.xre.silent-suppresses-warning`
- `tests.xre.special-symbol-not-harmonized`
- `tests.xre.special-symbol-warnings`
- `tests.xre.substitution-backtick`
