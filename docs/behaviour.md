# Behaviour rules

ares-compiler promises **reproducible output bytes** (`script.dat`/`script.idx`,
the js5 pack, `bin/<id>`) for a given source tree and `symbols.json`. Each rule
below names where it lives and what must not change. The committed snapshots
(`docs/testing.md`) pin all of it: a change that moves output bytes is a
deliberate re-bless, never a side effect.

## A. Crates

| crate | why | notes |
|---|---|---|
| `logos` | lexer | `String` mode via `morph`, `extras` for nesting depth |
| `chumsky` | parser over the token slice | `pratt` for condition/arithmetic precedence |
| `indexmap` | every insertion-ordered map/set that reaches output (B.3, B.4) | `serde` |
| `serde` / `serde_json` | `symbols.json`, test dumps | `preserve_order` (file order of `symbols.json` tables) |
| `flate2` | gzip of the js5 index group | `zlib`, no default features: stock C zlib (B.11) |
| `crc32fast` | the js5 group CRC32 (B.11) | |
| `ares-shared` | opcodes and pointer rules, triggers, `PointerType`, var type codes | shared with ares-gameserver; a change there moves both crates' output |

## B. Rules

**B.1 Strings are one byte per char.** `writer/binary.rs` `put_string`: the low
byte of each char's code point, then NUL (`€` → `0xAC`). A char whose low byte
is NUL (`Ā`, `😀`) is written as `?`, so it cannot end the string early. Not a
charset conversion.

**B.2 Source names are embedded in every script.** `compiler.rs` `SourceNames`:
`Absolute` (default) is the source root made absolute and lexically normalised
(`normalize_lexically`: `.`/`..` folded, OS separator) joined with the file's
path under it; `Relative` is the path relative to the source root, normalised
(machine-independent, used by the snapshots); `Prefixed` joins a fixed prefix with `/`, unnormalised.

**B.3 Walk order is bytewise.** `compiler.rs` `walk`: directory entries are
sorted by their byte names, directories inline. The order decides which
duplicate macro wins (first), which redeclared script wins, symbol-type first
insert order, and the order scripts are written to `bin/` and diagnostics.

**B.4 Symbol tables.** `runescript/type_info.rs` `ids`, `runescript/loaders.rs`:
every non-constant `symbols.json` key is a decimal `i32` (anything else is a
fatal error); tables load in id order, constants in file order. The id mapper
(`runescript/symbol_mapper.rs`) keeps the **first** insert for an id.
`symbol.rs`: symbol-type buckets are ordered by first insert and lookups take
the first match in that order. Supplier loaders (`param`, `varp`, `varn`,
`varo`, `vars`, `varbit`, `dbcolumn`) create a new type per symbol, so each symbol gets
its own bucket. Basic-symbol keys are lowercased with each whitespace run
(`char::is_whitespace`) replaced by `_`.

**B.5 Written values are range-checked.** `writer/binary.rs` `fit`: every u8,
i8, u16 and i32 field goes through `try_from`; a value that does not fit is a
`FatalError`, never truncated. That covers small-operand opcodes (u8), command
ids (u16), DEBUGPROC parameter type codes (u8; 0xff when the type has none),
counts (u16), the switch table count (u8) and jumps (i32). Switch keys that are
strings or longs are a fatal error. A command's operand is one byte, so a
command whose id is an opcode with a 4-byte operand (`large_operand`) is a
fatal error. The snapshot gates decode every written script with
`ares_shared::script::ScriptFile::decode`.

**B.6 Lookup key.** `writer/binary.rs` `lookup_key`, built by
`ares_shared::trigger::lookup_key` (the server looks scripts up with the same
function): `-1` for name-mode triggers; otherwise the trigger id (below 256)
`| kind << 8 | subject << 10` in `i32` (the shift wraps), kind 1 for a category
subject and 2 otherwise. Mapzone and coord subjects are their packed value.

**B.7 Coord, mapzone and zone packing.** `parser.rs` `pack_coord`:
`level_mx_mz_lx_lz` → `z | x << 14 | level << 28` with `x = mx << 6 | lx & 0x3fff`
(x is not masked); an oversized digit group wraps. `semantics/registration.rs`:
mapzone subjects `level_mx_mz` and zone subjects `level_mx_mz_lx_lz` parse
each part as `i32` (a non-number is a form error); zones round `lx`/`lz` with
`>> 3 << 3` and must be multiples of 8.

**B.8 Integer literals.** `semantics/typecheck.rs`: parsed as `i128`; decimal
must be within `i32`, hex and binary within `0..=0xffff_ffff` and then wrap to
`i32`. A leading `-` is part of the token. Long literals type-check but the
writer rejects `PushConstantLong`.

**B.9 Line-number table.** `writer/binary.rs` `write_script`: an entry is
added only for an instruction that carries a source location, and only when
its line differs from the previous sourced instruction. Which instructions
carry one is decided in `codegen/mod.rs`: branches from conditions, `if`/`while`
branches, default returns, discards, arithmetic ops and null string/long pushes
carry none. No `LineNumber` opcode is emitted.

**B.10 Macro preprocessor.** `preprocess.rs`, over `Vec<char>` (positions count
chars):
- a source with no `#` is returned unchanged (CRLF kept); otherwise `\r\n`
  becomes `\n` outside strings, chars and comments;
- `#NAME` / `#NAME(args)` consumes a trailing `;`; the expansion is collapsed
  onto the call-site line, so a macro must expand to whole statements;
- the first `#define` of a name wins, a redefinition is an error;
- arguments are not substituted inside string interpolations (`"<…>"`).
Source files are read as lossy UTF-8 with a leading BOM dropped (`compiler.rs`).

**B.11 js5 pack.** `writer/pack.rs`: only the index group is compressed (type
2: `p1 2, p4 compressed len, p4 len, gzip` with the gzip OS byte zeroed);
script groups are stored (type 0). CRC32 per group; `p_smart2or4` is the low
16 bits below 32768, else 4 bytes with the top bit set. The deflate stream is
stock zlib's at the default level. Jag `script.idx` counts ids `0..=last` with
size 0 for gaps; negative ids are never written to the jag but are in the js5.

**B.12 Ordering and determinism.** No hash-map iteration reaches output:
switch cases stay in source order (`codegen/switch_table.rs`), jag/js5 groups
are sorted by id, the pointer checker's graph keeps edge insertion order (it
decides which path and hints a pointer error reports). `str::to_lowercase` is
Unicode-aware.

**B.13 Compilation stops.** `compiler.rs`: a stage with errors stops the
compile and nothing is written. An empty command pointer table stops it
*without* errors and still writes empty packs.

**B.14 Diagnostics.** `diagnostics/mod.rs`: messages are `%s` templates
(`diagnostics/messages.rs`) filled by this crate's formatter. They are
compared loosely by the snapshot gates (count per type and file), so wording
may change without a re-bless of the byte snapshots.

**B.15 Client dialect.** `Config.dialect = Dialect::Client` compiles `.cs2`
files only (`compiler.rs`, by `Dialect::extension`; a server run compiles only
`.rs2`). Rules that reach the client script bytes:
- Triggers (`trigger.rs` `by_name`, `runescript/client_triggers.rs`): `command`,
  `proc` and `clientscript` (id 10, name mode, so lookup key `-1`); every other
  trigger is invalid in a client file, and `clientscript` is invalid in a server file.
- Commands are the caller's `symbols["command"]` table (Jagex client opcodes from
  `ares_shared::client_opcode`). Opcodes below 100 carry a 4-byte operand
  (`ares_shared::opcode::large_operand`), so `cc_create` (100) is a command with
  the one-byte operand; `1` selects the `.` form as on the server.
- `%name` takes the first game var named so in root-table order: `varp`, then
  `varbit`, then `varc` (the loader order). A varc is `varc<type>`; it reads and
  writes with `GET_VARC_INT`/`SET_VARC_INT` (42/43) or, for a string varc,
  `GET_VARC_STRING`/`SET_VARC_STRING` (49/50), operand = the varc id. `.%varc`
  is an error. Client varps and varbits need no pointer (`codegen/pointer.rs`).
- `event_*` (`runescript/loaders.rs` `EVENTS`) are constants `i32::MIN + n`:
  mousex 1, mousey 2, com 3, op 4, comsubid 5, com2 6, comsubid2 7, keycode 8,
  keychar 9; `event_opbase` is the string `"event_opbase"`.
- Hook setters (`cc_seton*`, `if_seton*` and their `.` forms; `runescript/commands.rs`
  `CommandHandler::Hook`) take `"script(args){triggers}"` or `null` as the first
  argument (`hook.rs`); anything else (a variable, a string with `<...>`) is an
  error. The script must be a `[clientscript]`. Pushed in order:
  the script id, each argument, then for a trigger list each trigger id and the
  count, then a descriptor string (each argument's type code, then `Y` when a
  trigger list was pushed), then the command's other arguments. `null` pushes
  `-1` and `""`. Trigger ids: varp id, varbit `id | 0x10000`, varc `id | 0x20000`
  (ares tags), inv id, stat id. Only `vartransmit` (vars), `invtransmit` (invs)
  and `stattransmit` (stats) take a list.

## C. Deliberate quirks that reach output

Kept on purpose; changing any of them moves output bytes.

| where | behaviour |
|---|---|
| `runescript/commands.rs` `expression_string` | `dump(...)` labels print nested expressions as `undefined`: `dump($x)` pushes `"$undefined="` |
| `runescript/commands.rs` (`dump`) | converts values with commands literally named `toString` / `escape`, skipped when absent |
| `runescript/commands.rs` (`script`) | pushes `"[trigger, name]"` **with a space** |
| `semantics/typecheck.rs` | string constants are re-parsed at `line - 1` (that line reaches the line table) |
| `runescript/symbol_mapper.rs` | an unknown script or command id is written as `-1` |
| `preprocess.rs` | a `.rs2` without `#` keeps its CRLF line endings |
