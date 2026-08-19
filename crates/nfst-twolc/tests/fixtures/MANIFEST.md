# twolc fixture corpus

## Real-world (18 files, ~8.3k lines)

Vendored from `/Users/brendan/git/divvun/`, except the omorfi file, which is
generated output from `github.com/flammie/omorfi`. Filenames encode the
source path so files with the same basename across repos don't collide.

The serious workout is the lang-sma/sme phonology files — each is over
1000 lines and exercises essentially every twolc construct that real
morphological grammars use.

| Vendored name | Source | Lines |
|---|---|---|
| `lang-sma__src__fst__morphology__phonology.twolc` | South Sami phonology | 1137 |
| `lang-sme__src__fst__morphology__phonology.twolc` | North Sami phonology | 1781 |
| `lang-sme__src__fst__morphology__phonology.bergslan.twolc` | North Sami (Bergsland variant) | 1781 |
| `lang-sme__src__fst__phonology-L2.twolc` | North Sami L2 phonology | 1763 |
| `lang-sme__src__fst__phonology-L2-from-branch.twolc` | L2 phonology variant | 1761 |
| `hfst__python__test__test{1,2,3}.twolc` (×3 repos) | HFST Python test grammars | 4 each |
| `hfst__scripts__windows_tests__test.twolc` (×3 repos) | HFST Windows tests | 4 each |
| `omorfi__src__generated__omorfi-hyphens.twolc` | omorfi hyphenation rules (`generate-twolcs.py -r hyphens`) | 42 |

The omorfi file is the regression case for divvun/hfst-rs#3: it is the only
real-world grammar here that uses the unparenthesised `where V in SetName`
form.

## Curated single-construct snippets

Hand-written to give each AST variant clean, isolated coverage:

- `snippet-where-matched.twolc`     — `where … matched`
- `snippet-where-mixed.twolc`       — `where … mixed`
- `snippet-except-clause.twolc`     — `except` after positive contexts
- `snippet-all-arrows.twolc`        — `=>`, `<=`, `<=>`, `/<=`
- `snippet-sets-and-defs.twolc`     — Sets + Definitions sections
- `snippet-multi-context.twolc`     — multi-context rule (each ends in `;`)
- `snippet-diacritics.twolc`        — Diacritics section
