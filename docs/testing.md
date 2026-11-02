# Snapshot tests

`tests/differential.rs` holds one `#[test]` per gate. Every gate runs
ares-compiler and compares the result with a committed snapshot under
`tests/`. The snapshots are the reference: they pin the rules in
`docs/behaviour.md`, so a change that moves output bytes, diagnostics or dumps
shows up as a failing gate.

```
cargo test --release                                  # unit tests + every gate
cargo test --release --test differential              # the gates
cargo test --release --test differential ir_matches   # one gate
```

## Updating snapshots (bless)

When a change is intentional, rewrite the snapshots from Rust output, review
the diff, and commit it with the change:

```
BLESS=1 cargo test --release --test differential
cargo test --release --test differential       # must be green
git diff tests/
```

`BLESS=1` rewrites only what mismatches, and inside a JSON snapshot only the
parts that differ (key order and layout are kept). `BLESS=all` rewrites every
snapshot even when it matches: `git diff` staying empty afterwards proves the
writer reproduces the files byte for byte. Gates read only fixtures and
corpus inputs, never another gate's snapshot, so blessing order does not
matter. `PIPELINE_SHOW` never blesses (it is a debugging run).

Never hand-edit a snapshot. Before relying on a new gate, break the behaviour
it covers once and check that the gate fails.

## Fixture snapshots (`tests/fixtures`, `tests/golden`)

`tests/fixtures/<case>/scripts/` and `symbols.json` are written by
`python -I tools/make_fixtures.py` from its `CASES` table (file names stay
ASCII-lowercase). `tests/golden/<case>/` holds:

| file | gate |
|---|---|
| `expanded/*.rs2`, `spans.json` | `expansion_matches` |
| `tokens.json` | `tokens_match` |
| `ast.json` | `ast_matches` |
| `ast-typed.json` | `typed_ast_matches` (absent when the case stops before type checking) |
| `ir.json` | `ir_matches` (absent likewise) |
| `diagnostics.json` | written by `bytecode_matches`; checked loosely (count per stage, type and file; syntax errors only by presence) by it, `typed_ast_matches` and `ir_matches` |
| `jag/script.dat`, `jag/script.idx`, `scripts.js5`, `bin/<id>` | `bytecode_matches`, byte for byte |

## Corpora

Each corpus is `tests/<dir>/inputs.json`, written by a seeded generator in
`tools/`, plus a snapshot. Regenerating inputs changes the snapshot, so bless
afterwards.

| dir | generator | snapshot | gate |
|---|---|---|---|
| `lexer` | `make_lexer_corpus.py` | `tokens.json` | `lexer_corpus_matches` |
| `parser` | `make_parser_corpus.py` | `trees.json` (FNV-1a hash of each tree, `null` on reject, `$fatal`) | `parser_corpus_matches` (`PARSER_SHOW=<i>` dumps a tree) |
| `macros` | `make_macro_corpus.py` | `expected.json` | `macro_corpus_matches` |
| `registration` | `make_register_corpus.py` | `expected.json` | `registration_corpus_matches` |
| `typecheck` | `make_typecheck_corpus.py` | `expected.json` | `typecheck_corpus_matches` (`TYPECHECK_SHOW=<i>`) |
| `pipeline` | `make_pipeline_corpus.py` | `expected.json` (IR and byte hashes per stage) | `pipeline_corpus_matches` (`PIPELINE_SHOW=<i>`) |
| `pointers` | `make_pointer_corpus.py` | `expected.json` | `pointer_corpus_matches` |
| `symbols` | hand-written `*.json` plus every fixture's `symbols.json` | `expected.json` | `symbols_match` |

Run generators from `ares-compiler/`, e.g. `python -I tools/make_parser_corpus.py`.

## Rendered diagnostics (`tests/render`)

`tests/render.rs` compiles each `tests/render/<case>/` together with
`tests/render/commands/` (or alone, when the case has its own `symbols.json`,
as `pointers` does for pointer rules) and compares
`Output::render_diagnostics()` (the rustc-style text ares prints) with the
case's `expected.txt`. `BLESS=1 cargo test --release --test render` rewrites
them. Add a case by adding a directory with its scripts and blessing.

## Real content

`content_matches` compiles a LostCity Content checkout and is skipped unless
both variables are set:

```
RS2_CORPUS=../Content RS2_CORPUS_GOLDEN=../content-golden cargo test --release --test differential content_matches
```

`RS2_CORPUS_GOLDEN` holds the input `symbols.json` and the snapshot
(`diagnostics.json` compared by count per stage and type, `jag/`,
`scripts.js5`, `bin/`). It lives outside the repo; bless it with the same
command plus `BLESS=1`.
