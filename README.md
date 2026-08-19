# nfst

Rust parsers, ASTs, and pretty-printers for the finite-state grammar
languages of the [HFST](https://hfst.github.io/) / Xerox toolchain.

Each language gets its own crate — a lexer, a typed AST, a recursive-descent
(or [`logos`](https://crates.io/crates/logos)-driven) parser, and a
round-trippable pretty-printer — all sharing `nfst-syntax` for source spans
and diagnostics.

## Crates

| Crate | Language |
| --- | --- |
| `nfst-syntax` | Shared source spans, `Spanned<T>`, and diagnostics |
| `nfst-xre` | Xerox regular expressions (xre) |
| `nfst-lexc` | `lexc` lexicon compiler |
| `nfst-twolc` | `twolc` two-level rules |
| `nfst-xfst` | `xfst` command scripts |
| `nfst-pmatch` | `pmatch` pattern matching |

## Versioning

Each crate is versioned and released independently. The languages are
separate, they track separate upstream grammars, and a fix to one of them
says nothing about the others — so the version numbers are allowed to
diverge rather than moving in lockstep.

## Status

Complete and used in production as part of a port of HFST. Each language
parses to a typed AST that round-trips losslessly through its
pretty-printer (`parse → print → parse` is structure-preserving).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
