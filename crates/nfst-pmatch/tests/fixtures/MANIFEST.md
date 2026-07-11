# pmatch fixture corpus

## `pmscript/` (15 files, ~890 lines)

Real-world Pmatch tokenisers vendored from the Giellatekno/Divvun
language repos. Filenames encode the source path (slashes → `__`) so
files with identical basenames (e.g. `tokenize-dog.pmscript` from
multiple repos) don't collide.

| Vendored name | Source |
|---|---|
| `lang-sma__tools__tokenisers__tokeniser-disamb-gt-desc.pmscript` | South Sami disambiguation tokeniser |
| `lang-sma__tools__tokenisers__tokeniser-gramcheck-gt-desc.pmscript` | South Sami grammar-check tokeniser |
| `lang-sma__tools__tokenisers__tokeniser-tts-cggt-desc.pmscript` | South Sami TTS tokeniser |
| `lang-sme__tools__tokenisers__tokeniser-disamb-gt-desc.pmscript` | North Sami disambiguation tokeniser |
| `lang-sme__tools__tokenisers__tokeniser-gramcheck-gt-desc.pmscript` | North Sami grammar-check tokeniser |
| `lang-sme__tools__tokenisers__tokeniser-tts-cggt-desc.pmscript` | North Sami TTS tokeniser |
| `lang-sme__tools__tokenisers__experimental__RC-with-flags.pmscript` | Experimental RC pattern |
| `lang-sme__tools__tokenisers__experimental__spacetag.pmscript` | Experimental spacetag pattern |
| `libdivvun__test__checker__tokeniser.pmscript` | Tokeniser used in libdivvun grammar-checker tests |
| `hfst__test__tools__tokenize-{dog,backtrack}.pmscript` | HFST tokenizer regression tests (×3 repos) |

## `snippets/` (23 files)

Small, hand-curated examples — one file per pmatch construct that the
upstream `pmatch-tests.sh` exercises. The shell-script extraction was
out of scope; these snippets target each AST variant directly so the
parse-only port has clean per-construct coverage even on cases that
real-world tokenisers don't exercise.

Each fixture is named after the construct it focuses on:

- `comment-only-line.pmatch`, `comment-at-eof.pmatch`
- `define-with-zero-in-name.pmatch` — symbols that contain `0`
- `rc-no-separator.pmatch`, `different-tag-different-context.pmatch`
- `ins-with-substring-name.pmatch`, `defins.pmatch`
- `cap-with-side.pmatch`, `like-with-threshold.pmatch`
- `substitute.pmatch`, `uncompose.pmatch`
- `character-range.pmatch`, `all-acceptors.pmatch`
- `function-definition.pmatch`, `regex-top-level.pmatch`
- `with-tag.pmatch`, `or-and-contexts.pmatch`
- `lst-and-exc.pmatch`, `counter.pmatch`
- `capture-and-end-tag.pmatch`
- `list-definition.pmatch`
- `explode-implode.pmatch`, `sigma.pmatch`
