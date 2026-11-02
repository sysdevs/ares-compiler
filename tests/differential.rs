//! Snapshot gates: every stage of the pipeline compared against committed
//! goldens, which are frozen ares-compiler output.
//!
//! Layout:
//!   tests/fixtures/<case>/scripts/*.rs2   input (also `.macro`)
//!   tests/fixtures/<case>/symbols.json    CompilerTypeInfo tables
//!   tests/golden/<case>/...               per-stage snapshots of that case
//!   tests/<corpus>/inputs.json            edge-case corpora (written by tools/make_*_corpus.py)
//!   tests/<corpus>/expected.json          their snapshots
//!
//! After an intentional output change:
//!   BLESS=1 cargo test --test differential
//! rewrites the snapshots that mismatch (only the parts that differ);
//! `BLESS=all` rewrites every snapshot and must leave `git diff tests/` empty.
//! Review the diff before committing. See `docs/testing.md`.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn cases() -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(root().join("tests/fixtures"))
        .expect("tests/fixtures missing")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn golden(case: &str, file: &str) -> PathBuf {
    root().join("tests/golden").join(case).join(file)
}

fn read_json(p: &Path) -> Value {
    let text = fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// `tests/<dir>/<file>`.
fn corpus(dir: &str, file: &str) -> PathBuf {
    root().join("tests").join(dir).join(file)
}

/// A fixture's source files in compile order (bytewise, as the compiler walks).
fn walk(case: &str) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(root().join("tests/fixtures").join(case).join("scripts"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// Every `.rs2` of a fixture after macro expansion: (file, text, spans).
fn expand_case(case: &str) -> Vec<(String, String, Vec<ares_compiler::preprocess::MacroExpansionSpan>)> {
    use ares_compiler::preprocess::{expand_source_with_map, parse_macro_file, MacroRegistry};
    let walk = walk(case);
    let read = |f: &str| String::from_utf8_lossy(&fs::read(root().join("tests/fixtures").join(case).join("scripts").join(f)).unwrap()).into_owned();
    let (mut registry, mut diags) = (MacroRegistry::default(), Vec::new());
    for f in walk.iter().filter(|f| f.ends_with(".macro")) {
        parse_macro_file(f, &read(f), &mut registry, &mut diags);
    }
    walk.iter()
        .filter(|f| f.ends_with(".rs2"))
        .map(|f| {
            let (text, spans) = expand_source_with_map(&read(f), &registry, &mut diags, f);
            (f.clone(), text, spans)
        })
        .collect()
}

/// A hex window around the first mismatch, or `None` when equal.
fn bytes_diff(what: &str, expected: &[u8], actual: &[u8]) -> Option<String> {
    if expected == actual {
        return None;
    }
    let first = expected.iter().zip(actual).position(|(a, b)| a != b).unwrap_or(expected.len().min(actual.len()));
    let lo = first.saturating_sub(16);
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ");
    Some(format!(
        "{what}: first mismatch at offset {first} (expected len {}, actual len {})\n expected: {}\n actual:   {}",
        expected.len(),
        actual.len(),
        hex(&expected[lo..expected.len().min(first + 16)]),
        hex(&actual[lo..actual.len().min(first + 16)]),
    ))
}

/// Collects every failure and reports the first few.
fn report(what: &str, failures: Vec<String>) {
    if !failures.is_empty() {
        let first: Vec<_> = failures.iter().take(5).cloned().collect();
        panic!("{what}: {} mismatches, first 5:\n{}", failures.len(), first.join("\n"));
    }
}

/// `BLESS` unset: `None`; `BLESS=all`: `Some(true)` (always rewrite); any other value: `Some(false)`.
fn bless() -> Option<bool> {
    std::env::var("BLESS").ok().map(|v| v == "all")
}

/// Rewrites `e` to equal `a`, keeping `e`'s key order and every part already equal.
fn patch(e: &mut Value, a: &Value) {
    if e == a {
        return;
    }
    match (e, a) {
        (Value::Object(eo), Value::Object(ao)) => {
            eo.retain(|k, _| ao.contains_key(k));
            for (k, v) in ao {
                match eo.get_mut(k) {
                    Some(ev) => patch(ev, v),
                    None => {
                        eo.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (Value::Array(ea), Value::Array(aa)) => {
            ea.truncate(aa.len());
            for (i, v) in aa.iter().enumerate() {
                match ea.get_mut(i) {
                    Some(ev) => patch(ev, v),
                    None => ea.push(v.clone()),
                }
            }
        }
        (e, a) => *e = a.clone(),
    }
}

/// Writes JSON in the layout of the file it replaces: one-space indented, one
/// array item per line (`[\n{..},\n{..}\n]` or `[{..},\n{..}]`), or compact; a
/// trailing LF is kept when the old file had one (new files get one-space indent + LF).
fn write_json(path: &Path, v: &Value) {
    use serde::Serialize;
    let old = fs::read(path).ok();
    let b = old.as_deref().unwrap_or(b"[\n ");
    let pretty = (b.get(1) == Some(&b'\n') && b.get(2) == Some(&b' ')) || b.starts_with(b"[]") || b.starts_with(b"{}");
    let mut out = Vec::new();
    if !pretty && v.is_array() && b.windows(2).any(|w| w == b",\n") {
        let nl = if b.get(1) == Some(&b'\n') { "\n" } else { "" };
        let items: Vec<String> = v.as_array().unwrap().iter().map(|x| x.to_string()).collect();
        out.extend(format!("[{nl}{}{nl}]", items.join(",\n")).bytes());
    } else if pretty {
        v.serialize(&mut serde_json::Serializer::with_formatter(&mut out, serde_json::ser::PrettyFormatter::with_indent(b" "))).unwrap();
    } else {
        serde_json::to_writer(&mut out, v).unwrap();
    }
    if old.as_ref().is_none_or(|o| o.ends_with(b"\n")) {
        out.push(b'\n');
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(path, out).unwrap();
}

/// Under `BLESS` writes the patched expected value (on a mismatch, or always for `BLESS=all`); otherwise reports.
fn settle(what: &str, path: &Path, e: &Value, failures: Vec<String>) {
    match bless() {
        Some(all) if all || !failures.is_empty() => write_json(path, e),
        Some(_) => {}
        None => report(what, failures),
    }
}

/// Compares a raw snapshot file; under `BLESS` writes `actual` instead of failing.
fn settle_bytes(what: &str, path: &Path, actual: &[u8], failures: &mut Vec<String>) {
    let expected = fs::read(path).unwrap_or_default();
    match bless() {
        Some(all) if all || expected != actual => {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, actual).unwrap();
        }
        Some(_) => {}
        None => failures.extend(bytes_diff(what, &expected, actual)),
    }
}

/// Compares one JSON item against `e` and patches `e` on a mismatch.
fn check(what: String, e: &mut Value, a: &Value, failures: &mut Vec<String>) {
    if e != a {
        failures.push(format!("{what}\n{}", first_json_diff(e, a, "$")));
        patch(e, a);
    }
}

/// Compares item by item (`what(i)` names item `i`), then the lengths.
fn check_items(what: impl Fn(usize) -> String, e: &mut Value, a: Vec<Value>, failures: &mut Vec<String>) {
    let ea = e.as_array().map_or(0, |x| x.len());
    for (i, (ev, av)) in e.as_array_mut().into_iter().flatten().zip(&a).enumerate() {
        check(what(i), ev, av, failures);
    }
    if ea != a.len() {
        failures.push(format!("{}: {ea} items expected, {} actual", what(usize::MAX), a.len()));
        patch(e, &Value::Array(a));
    }
}

#[test]
fn tokens_match() {
    for case in cases() {
        let path = golden(&case, "tokens.json");
        let mut expected = read_json(&path);
        let mut actual = serde_json::Map::new();
        for (file, text, _) in expand_case(&case) {
            let tokens = ares_compiler::lexer::tokenize(&text).tokens;
            let tokens: Vec<_> = tokens
                .iter()
                .map(|t| json!({ "type": t.kind, "text": t.text, "line": t.line, "column": t.column, "channel": t.channel }))
                .collect();
            actual.insert(file, json!(tokens));
        }
        let mut failures = Vec::new();
        check(format!("{case}/tokens.json"), &mut expected, &Value::Object(actual), &mut failures);
        settle(&format!("{case} tokens"), &path, &expected, failures);
    }
}

/// Lexer edge-case corpus (`tests/lexer/`): tokens and lexer errors.
#[test]
fn lexer_corpus_matches() {
    let inputs = read_json(&corpus("lexer", "inputs.json"));
    let path = corpus("lexer", "tokens.json");
    let mut expected = read_json(&path);
    let actual: Vec<Value> = inputs
        .as_array()
        .unwrap()
        .iter()
        .map(|input| {
            let ares_compiler::lexer::Lexed { tokens, errors, .. } = ares_compiler::lexer::tokenize(input.as_str().unwrap());
            json!({
                "tokens": tokens.iter().map(|t| json!([t.kind, t.text, t.line, t.column, t.channel])).collect::<Vec<_>>(),
                "errors": errors.iter().map(|e| json!({"line": e.line, "column": e.column, "msg": e.message()})).collect::<Vec<_>>(),
            })
        })
        .collect();
    let mut failures = Vec::new();
    check_items(|i| format!("input #{i} {:?}", inputs.get(i)), &mut expected, actual, &mut failures);
    settle("lexer corpus", &path, &expected, failures);
}

/// AST (post-parse, pre-semantic) of every file that parsed.
#[test]
fn ast_matches() {
    for case in cases() {
        let path = golden(&case, "ast.json");
        let mut expected = read_json(&path);
        let mut actual = Vec::new();
        for (file, text, _) in expand_case(&case) {
            if let Some(node) = ares_compiler::parser::parse_script_file(&text, &file).unwrap().node {
                actual.push(empty_dump(|d| d.script_file(&node)));
            }
        }
        let mut failures = Vec::new();
        check_items(|i| format!("{case}: ast file #{i}"), &mut expected, actual, &mut failures);
        settle(&format!("{case} ast"), &path, &expected, failures);
    }
}

/// Parser corpus (`tests/parser/`). Per input: `null` when the parser rejects
/// it, `{"$fatal": msg}` on a fatal error, else the FNV-1a 64 hash of the
/// tree's sorted-key JSON. `PARSER_SHOW=<index>` writes that tree to the temp dir.
#[test]
fn parser_corpus_matches() {
    use ares_compiler::parser::{parse_script_file, parse_single_expression};
    let inputs = read_json(&corpus("parser", "inputs.json"));
    let path = corpus("parser", "trees.json");
    let mut expected = read_json(&path);
    let actual: Vec<Value> = inputs
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, input)| {
            let text = input["text"].as_str().unwrap();
            let tree = match input["entry"].as_str().unwrap() {
                "file" => parse_script_file(text, "case.rs2").map(|p| p.node.map(|n| empty_dump(|d| d.script_file(&n)))),
                _ => parse_single_expression(text, "case.rs2", 3, 7).map(|p| p.node.map(|n| empty_dump(|d| d.expression(&n)))),
            };
            if std::env::var("PARSER_SHOW").is_ok_and(|v| v == i.to_string()) {
                let path = std::env::temp_dir().join("parser_show.json");
                fs::write(&path, serde_json::to_string_pretty(&tree.as_ref().ok()).unwrap()).unwrap();
                println!("tree #{i} written to {}", path.display());
            }
            match tree {
                Err(fatal) => json!({ "$fatal": fatal.0 }),
                Ok(None) => Value::Null,
                Ok(Some(tree)) => Value::String(fnv1a64(&canonical_json(&tree))),
            }
        })
        .collect();
    let mut failures = Vec::new();
    check_items(|i| format!("#{i} {}", inputs.get(i).map_or(Value::Null, |x| x.clone())), &mut expected, actual, &mut failures);
    settle("parser corpus", &path, &expected, failures);
}

/// JSON with object keys sorted.
fn canonical_json(v: &Value) -> String {
    match v {
        Value::Array(a) => format!("[{}]", a.iter().map(canonical_json).collect::<Vec<_>>().join(",")),
        Value::Object(o) => {
            let mut keys: Vec<_> = o.keys().collect();
            keys.sort();
            let fields: Vec<_> = keys.iter().map(|k| format!("{}:{}", serde_json::to_string(k).unwrap(), canonical_json(&o[*k]))).collect();
            format!("{{{}}}", fields.join(","))
        }
        _ => serde_json::to_string(v).unwrap(),
    }
}

fn fnv1a64(s: &str) -> String {
    let h = s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3));
    format!("{h:016x}")
}

/// Dumps an untyped tree (no types or symbols to resolve).
fn empty_dump(f: impl FnOnce(ares_compiler::ast::dump::Dump) -> Value) -> Value {
    let (types, tables) = (ares_compiler::types::TypeManager::new(), ares_compiler::symbol::SymbolTables::default());
    f(ares_compiler::ast::dump::Dump { types: &types, tables: &tables })
}

/// The front end for one fixture, up to and including script registration:
/// special symbols (checkPointers on), symbol loading, parsing every expanded
/// `.rs2` in walk order, registration and secondary commands. Returns the files that parsed.
fn register_fixture(
    case: &str,
) -> (ares_compiler::runescript::compiler::ServerScriptCompiler, Vec<ares_compiler::ast::ScriptFile>, ares_compiler::diagnostics::Diagnostics) {
    use indexmap::IndexMap;
    use ares_compiler::runescript::compiler::{load_special_symbols, ServerScriptCompiler};
    use ares_compiler::runescript::type_info::CompilerTypeInfo;
    let symbols: IndexMap<String, CompilerTypeInfo> =
        serde_json::from_str(&fs::read_to_string(root().join("tests/fixtures").join(case).join("symbols.json")).unwrap()).unwrap();
    let mut mapper = Default::default();
    let pointers = load_special_symbols(&symbols["command"], &symbols["runescript"], &mut mapper, true).unwrap();
    let mut c = ServerScriptCompiler::new(symbols, mapper, pointers, Default::default()).unwrap();
    c.load_symbols().unwrap();
    let mut files = Vec::new();
    for (f, text, _) in expand_case(case) {
        files.extend(ares_compiler::parser::parse_script_file(&text, &f).unwrap().node);
    }
    let mut diagnostics = Default::default();
    ares_compiler::semantics::registration::register_scripts(&mut c, &mut files, &mut diagnostics);
    c.register_secondary_commands();
    (c, files, diagnostics)
}

/// Path and values of the first difference between two JSON trees.
fn first_json_diff(e: &Value, a: &Value, path: &str) -> String {
    use serde_json::Value::*;
    match (e, a) {
        (Object(eo), Object(ao)) => {
            for (k, ev) in eo {
                match ao.get(k) {
                    None => return format!("{path}.{k}: missing (expected {ev})"),
                    Some(av) if av != ev => return first_json_diff(ev, av, &format!("{path}.{k}")),
                    _ => {}
                }
            }
            match ao.keys().find(|k| !eo.contains_key(*k)) {
                Some(k) => format!("{path}.{k}: unexpected {}", ao[k]),
                None => format!("{path}: differs"),
            }
        }
        (Array(ea), Array(aa)) if ea.len() == aa.len() => {
            let i = ea.iter().zip(aa).position(|(x, y)| x != y).unwrap();
            first_json_diff(&ea[i], &aa[i], &format!("{path}[{i}]"))
        }
        _ => format!("{path}:\n expected: {e}\n actual:   {a}"),
    }
}

/// Macro expansion text (`expanded/`) and spans.
#[test]
fn expansion_matches() {
    for case in cases() {
        let path = golden(&case, "spans.json");
        let mut spans = read_json(&path);
        let mut actual = serde_json::Map::new();
        let mut failures = Vec::new();
        for (f, text, s) in expand_case(&case) {
            settle_bytes(&format!("{case}/expanded/{f}"), &golden(&case, &format!("expanded/{f}")), text.as_bytes(), &mut failures);
            actual.insert(f, serde_json::to_value(&s).unwrap());
        }
        check(format!("{case}/spans.json"), &mut spans, &Value::Object(actual), &mut failures);
        settle(&format!("{case} expansion"), &path, &spans, failures);
    }
}

/// Typed AST (types, symbol references) after type checking. Cases that stop
/// before type checking have no `ast-typed.json`.
#[test]
fn typed_ast_matches() {
    for case in cases() {
        let path = golden(&case, "ast-typed.json");
        if !path.exists() {
            continue;
        }
        let (mut c, mut files, mut diagnostics) = register_fixture(&case);
        ares_compiler::semantics::typecheck::type_check(&mut c, &mut files, &mut diagnostics).unwrap();
        let dump = ares_compiler::ast::dump::Dump { types: &c.types, tables: &c.tables };
        let mut expected = read_json(&path);
        let mut failures = Vec::new();
        check_items(|i| format!("{case}: typed file #{i}"), &mut expected, files.iter().map(|f| dump.script_file(f)).collect(), &mut failures);
        settle(&format!("{case} typed ast"), &path, &expected, failures);
        if bless().is_none() {
            assert_loose_diagnostics(&case, "typecheck", &diagnostics);
        }
    }
}

/// Loose diagnostics comparison: same count per (type, file) for one stage,
/// except `SYNTAX_ERROR`, which only has to be present in the same files.
fn assert_loose_diagnostics(case: &str, stage: &str, actual: &ares_compiler::diagnostics::Diagnostics) {
    if let Some(e) = loose_diagnostics_diff(case, stage, actual) {
        panic!("{e}");
    }
}

fn loose_diagnostics_diff(case: &str, stage: &str, actual: &ares_compiler::diagnostics::Diagnostics) -> Option<String> {
    use std::collections::BTreeMap;
    let key = |kind: &str, file: &str| (kind.to_owned(), file.to_owned());
    let mut expected: BTreeMap<(String, String), usize> = BTreeMap::new();
    for d in read_json(&golden(case, "diagnostics.json")).as_array().unwrap().iter().filter(|d| d["stage"] == stage) {
        *expected.entry(key(d["type"].as_str().unwrap(), d["name"].as_str().unwrap())).or_default() += 1;
    }
    let mut got: BTreeMap<(String, String), usize> = BTreeMap::new();
    for d in &actual.list {
        let kind = serde_json::to_value(d.kind).unwrap();
        *got.entry(key(kind.as_str().unwrap(), &d.source.name)).or_default() += 1;
    }
    for m in [&mut expected, &mut got] {
        for ((kind, _), n) in m.iter_mut() {
            if kind == "SYNTAX_ERROR" {
                *n = 1;
            }
        }
    }
    let formatted: Vec<String> = actual.list.iter().map(|d| format!("{}:{}: {}", d.source.name, d.source.line, d.formatted())).collect();
    (expected != got).then(|| format!("{case}: {stage} diagnostics\n expected: {expected:?}\n actual:   {got:?}\n ours: {formatted:#?}"))
}

/// IR after code generation.
#[test]
fn ir_matches() {
    for case in cases() {
        let path = golden(&case, "ir.json");
        if !path.exists() {
            continue;
        }
        let (mut c, mut files, mut diagnostics) = register_fixture(&case);
        ares_compiler::semantics::typecheck::type_check(&mut c, &mut files, &mut diagnostics).unwrap();
        let mut codegen_diagnostics = Default::default();
        let scripts = ares_compiler::codegen::generate(&c, &files, &mut codegen_diagnostics).unwrap();
        let mut expected = read_json(&path);
        let actual = scripts.iter().map(|s| ares_compiler::codegen::ir::dump(s, &c.types, &c.tables)).collect();
        let mut failures = Vec::new();
        check_items(|i| format!("{case}: script #{i}"), &mut expected, actual, &mut failures);
        settle(&format!("{case} ir"), &path, &expected, failures);
        if bless().is_none() {
            assert_loose_diagnostics(&case, "codegen", &codegen_diagnostics);
        }
    }
}

fn stage_name(stage: ares_compiler::compiler::Stage) -> &'static str {
    use ares_compiler::compiler::Stage;
    match stage {
        Stage::Parse => "parse",
        Stage::Analyze => "typecheck",
        Stage::Codegen => "codegen",
        Stage::Pointers => "pointers",
    }
}

/// Every diagnostic of a compile, in the `diagnostics.json` shape.
fn diagnostics_json(output: &ares_compiler::compiler::Output) -> Value {
    output
        .diagnostics
        .iter()
        .flat_map(|(stage, d)| d.list.iter().map(move |d| (stage, d)))
        .map(|(stage, d)| {
            json!({
                "stage": stage_name(*stage), "type": d.kind, "name": d.source.name,
                "line": d.source.line, "column": d.source.column, "message": d.formatted(),
            })
        })
        .collect()
}

/// Compares (or under `BLESS` writes) `jag/`, `scripts.js5` and `bin/` under `dir`.
fn settle_packs(what: &str, dir: &Path, packs: &ares_compiler::writer::pack::Packs, failures: &mut Vec<String>) {
    settle_bytes(&format!("{what}/jag/script.dat"), &dir.join("jag/script.dat"), &packs.script_dat, failures);
    settle_bytes(&format!("{what}/jag/script.idx"), &dir.join("jag/script.idx"), &packs.script_idx, failures);
    settle_bytes(&format!("{what}/scripts.js5"), &dir.join("scripts.js5"), &packs.js5, failures);
    let mut bin: Vec<String> =
        fs::read_dir(dir.join("bin")).map(|d| d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    bin.sort();
    let mut ours: Vec<String> = packs.bin.keys().cloned().collect();
    ours.sort();
    if bin != ours {
        if bless().is_some() {
            for stale in bin.iter().filter(|b| !packs.bin.contains_key(*b)) {
                fs::remove_file(dir.join("bin").join(stale)).unwrap();
            }
        } else {
            failures.push(format!("{what}: bin/ file names\n expected: {bin:?}\n actual:   {ours:?}"));
        }
    }
    for (name, data) in &packs.bin {
        // The server's decoder must read back every script we write.
        if let Err(e) = ares_shared::script::ScriptFile::decode(0, data.clone()) {
            failures.push(format!("{what}/bin/{name}: does not decode: {e}"));
        }
        settle_bytes(&format!("{what}/bin/{name}"), &dir.join("bin").join(name), data, failures);
    }
}

/// Whole-compile snapshots: packs, per-script binaries and diagnostics.
#[test]
fn bytecode_matches() {
    use indexmap::IndexMap;
    use ares_compiler::compiler::SourceNames;
    use ares_compiler::runescript::type_info::CompilerTypeInfo;
    for case in cases() {
        let symbols: IndexMap<String, CompilerTypeInfo> =
            serde_json::from_str(&fs::read_to_string(root().join("tests/fixtures").join(&case).join("symbols.json")).unwrap()).unwrap();
        let config = ares_compiler::Config {
            source_paths: vec![root().join("tests/fixtures").join(&case).join("scripts")],
            symbols,
            source_names: SourceNames::Relative,
            ..Default::default()
        };
        let output = ares_compiler::compile(&config).unwrap();
        let mut failures = Vec::new();
        let diag_path = golden(&case, "diagnostics.json");
        let loose: Vec<String> = output.diagnostics.iter().filter_map(|(stage, d)| loose_diagnostics_diff(&case, stage_name(*stage), d)).collect();
        match bless() {
            Some(all) if all || !loose.is_empty() => write_json(&diag_path, &diagnostics_json(&output)),
            Some(_) => {}
            None => failures.extend(loose),
        }
        // A stopped compile still writes (empty) packs.
        let packs = output.packs.unwrap_or_else(|| ares_compiler::writer::pack::pack(&[]).unwrap());
        settle_packs(&case, &golden(&case, ""), &packs, &mut failures);
        report(&format!("{case} bytecode"), failures);
    }
}

/// Exclude paths match a script's real file, also with relative source names.
#[test]
fn exclude_matches_real_paths() {
    use indexmap::IndexMap;
    use ares_compiler::compiler::SourceNames;
    use ares_compiler::runescript::type_info::CompilerTypeInfo;
    let dir = root().join("tests/fixtures/06-procs-labels");
    let symbols: IndexMap<String, CompilerTypeInfo> = serde_json::from_str(&fs::read_to_string(dir.join("symbols.json")).unwrap()).unwrap();
    let compile = |exclude: Vec<PathBuf>| {
        let config = ares_compiler::Config {
            source_paths: vec![dir.join("scripts")],
            exclude_paths: exclude,
            symbols: symbols.clone(),
            source_names: SourceNames::Relative,
            ..Default::default()
        };
        ares_compiler::compile(&config).unwrap().scripts_written
    };
    let all = compile(vec![]);
    assert!(all > 0);
    assert_eq!(compile(vec![dir.join("scripts/procs.rs2")]), 0);
}

/// Real-content gate, skipped unless `RS2_CORPUS` (a LostCity Content
/// checkout) and `RS2_CORPUS_GOLDEN` (its snapshot directory, holding the
/// input `symbols.json`) are set. Diagnostics are compared by count per stage
/// and type; the packs and every script binary byte for byte.
#[test]
fn content_matches() {
    use indexmap::IndexMap;
    use ares_compiler::compiler::SourceNames;
    use ares_compiler::runescript::type_info::CompilerTypeInfo;
    let (Some(content), Some(golden)) = (std::env::var_os("RS2_CORPUS"), std::env::var_os("RS2_CORPUS_GOLDEN")) else {
        println!("content_matches skipped: set RS2_CORPUS and RS2_CORPUS_GOLDEN");
        return;
    };
    let (content, golden) = (PathBuf::from(content), PathBuf::from(golden));
    let symbols: IndexMap<String, CompilerTypeInfo> = serde_json::from_str(&fs::read_to_string(golden.join("symbols.json")).unwrap()).unwrap();
    let config = ares_compiler::Config {
        source_paths: vec![content.join("scripts")],
        symbols,
        source_names: SourceNames::Relative,
        ..Default::default()
    };
    let output = ares_compiler::compile(&config).unwrap();
    let counts = |list: &[Value]| {
        let mut m = std::collections::BTreeMap::new();
        for d in list {
            *m.entry((d["stage"].as_str().unwrap().to_owned(), d["type"].as_str().unwrap().to_owned())).or_insert(0) += 1;
        }
        m
    };
    let actual = diagnostics_json(&output);
    let diag_path = golden.join("diagnostics.json");
    let (want, got) = (counts(read_json(&diag_path).as_array().unwrap()), counts(actual.as_array().unwrap()));
    let mut failures = Vec::new();
    match bless() {
        Some(all) if all || want != got => write_json(&diag_path, &actual),
        Some(_) => {}
        None if want != got => failures.push(format!("diagnostic counts per (stage, type)\n expected: {want:?}\n actual:   {got:?}")),
        None => {}
    }
    let packs = output.packs.unwrap_or_else(|| ares_compiler::writer::pack::pack(&[]).unwrap());
    settle_packs("content", &golden, &packs, &mut failures);
    report("content", failures);
    println!("content: {} scripts match", packs.bin.len());
}

/// Symbol loading (`tests/symbols/`). Builds the compiler for every fixture's
/// `symbols.json` plus the edge files, loads symbols, and compares the root
/// table (SymbolType order, keys, symbols, mapper ids), the registered type
/// names, or the fatal error message.
#[test]
fn symbols_match() {
    use indexmap::IndexMap;
    use ares_compiler::runescript::compiler::ServerScriptCompiler;
    use ares_compiler::runescript::type_info::CompilerTypeInfo;
    use ares_compiler::symbol::{Symbol, SymbolTables, SymbolType};

    let path = root().join("tests/symbols/expected.json");
    let mut expected = read_json(&path);
    let mut files: Vec<String> = cases().iter().map(|c| format!("tests/fixtures/{c}/symbols.json")).collect();
    let mut edge: Vec<String> = fs::read_dir(root().join("tests/symbols"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|f| f != "expected.json")
        .map(|f| format!("tests/symbols/{f}"))
        .collect();
    edge.sort();
    files.extend(edge);
    let mut actual = serde_json::Map::new();
    for file in files {
        let symbols: IndexMap<String, CompilerTypeInfo> = serde_json::from_str(&fs::read_to_string(root().join(&file)).unwrap()).unwrap();
        let built = ServerScriptCompiler::new(symbols, Default::default(), Default::default(), Default::default())
            .and_then(|mut c| c.load_symbols().map(|_| c));
        let result = match built {
            Err(e) => json!({ "fatal": e.0 }),
            Ok(c) => {
                let table: Vec<Value> = c
                    .tables
                    .buckets(SymbolTables::ROOT)
                    .map(|(ty, entries)| {
                        let (kind, ty_name) = match ty {
                            SymbolType::ServerScript(t) => ("ServerScript", json!(t.get().identifier)),
                            SymbolType::LocalVariable => ("LocalVariable", Value::Null),
                            SymbolType::Basic(t) => ("Basic", json!(c.types[t].representation)),
                            SymbolType::Constant => ("Constant", Value::Null),
                        };
                        let entries: Vec<Value> = entries
                            .map(|(key, id)| {
                                let sym = match &c.tables[id] {
                                    Symbol::Constant { name, value } => json!({ "name": name, "value": value }),
                                    Symbol::Basic { name, ty, is_protected } => {
                                        json!({ "name": name, "type": c.types[*ty].representation, "isProtected": is_protected })
                                    }
                                    other => panic!("unexpected loaded symbol {other:?}"),
                                };
                                let id = match c.mapper.get(&c.tables, id) {
                                    Err(_) => json!("none"),
                                    Ok(n) => json!(n),
                                };
                                json!([key, sym, id])
                            })
                            .collect();
                        json!({ "kind": kind, "type": ty_name, "entries": entries })
                    })
                    .collect();
                // Sorted by their "name,repr" strings.
                let mut types: Vec<(String, &str, String)> =
                    c.types.names().map(|(n, t)| (format!("{n},{}", c.types[t].representation), n, c.types[t].representation.clone())).collect();
                types.sort();
                let types: Vec<Value> = types.into_iter().map(|(_, n, r)| json!([n, r])).collect();
                json!({ "table": table, "types": types })
            }
        };
        actual.insert(file, result);
    }
    let mut failures = Vec::new();
    for (file, a) in &actual {
        match expected.get_mut(file) {
            Some(e) => check(format!("{file}: symbols"), e, a, &mut failures),
            None => failures.push(format!("{file}: no snapshot")),
        }
    }
    patch(&mut expected, &Value::Object(actual));
    settle("symbols", &path, &expected, failures);
}

/// Macro preprocessor corpus (`tests/macros/`): registry contents and
/// definition diagnostics, then per source the expanded text, spans and diagnostics.
#[test]
fn macro_corpus_matches() {
    use ares_compiler::preprocess::{expand_source_with_map, parse_macro_file, MacroDiagnostic, MacroRegistry};
    let diags = |d: &[MacroDiagnostic]| -> Value { d.iter().map(|d| json!([d.file, d.line, d.column, d.message])).collect() };
    let inputs = read_json(&corpus("macros", "inputs.json"));
    let path = corpus("macros", "expected.json");
    let mut expected = read_json(&path);
    let actual: Vec<Value> = inputs
        .as_array()
        .unwrap()
        .iter()
        .map(|input| {
            let (mut registry, mut defs) = (MacroRegistry::default(), Vec::new());
            for m in input["macros"].as_array().unwrap() {
                parse_macro_file(m["file"].as_str().unwrap(), m["text"].as_str().unwrap(), &mut registry, &mut defs);
            }
            let mut macros: Vec<_> = registry.macros.values().collect();
            macros.sort_by(|a, b| a.name.cmp(&b.name));
            let macros: Value = macros.iter().map(|m| json!([m.name, m.file, m.line, m.args(), m.body()])).collect();
            let sources: Value = input["sources"]
                .as_array()
                .unwrap()
                .iter()
                .map(|src| {
                    let mut d = Vec::new();
                    let (text, spans) = expand_source_with_map(src["text"].as_str().unwrap(), &registry, &mut d, src["name"].as_str().unwrap());
                    json!({ "text": text, "spans": spans, "diagnostics": diags(&d) })
                })
                .collect();
            json!({ "macros": macros, "definitionDiagnostics": diags(&defs), "sources": sources })
        })
        .collect();
    let mut failures = Vec::new();
    check_items(|i| format!("case #{i}"), &mut expected, actual, &mut failures);
    settle("macro corpus", &path, &expected, failures);
}

/// Script registration corpus (`tests/registration/`): per case the
/// Script-level fields, parameter symbols, root-table script buckets (in
/// order) and every registration diagnostic.
#[test]
fn registration_corpus_matches() {
    use indexmap::IndexMap;
    use ares_compiler::runescript::compiler::{load_special_symbols, ServerScriptCompiler};
    use ares_compiler::runescript::type_info::CompilerTypeInfo;
    use ares_compiler::symbol::{Symbol, SymbolId, SymbolTables, SymbolType};

    let inputs = read_json(&corpus("registration", "inputs.json"));
    let path = corpus("registration", "expected.json");
    let mut expected = read_json(&path);
    let actual: Vec<Value> = inputs
        .as_array()
        .unwrap()
        .iter()
        .map(|input| {
            let symbols: IndexMap<String, CompilerTypeInfo> = serde_json::from_value(input["symbols"].clone()).unwrap();
            let mut mapper = Default::default();
            let pointers = load_special_symbols(&symbols["command"], &symbols["runescript"], &mut mapper, true).unwrap();
            let features = serde_json::from_value(input["features"].clone()).unwrap();
            let mut c = ServerScriptCompiler::new(symbols, mapper, pointers, features).unwrap();
            c.load_symbols().unwrap();
            let (mut files, mut parsed) = (Vec::new(), Vec::new());
            for f in input["files"].as_array().unwrap() {
                let node = ares_compiler::parser::parse_script_file(f["text"].as_str().unwrap(), f["name"].as_str().unwrap()).unwrap().node;
                parsed.push(node.is_some());
                files.extend(node);
            }
            let mut diagnostics = Default::default();
            ares_compiler::semantics::registration::register_scripts(&mut c, &mut files, &mut diagnostics);
            c.register_secondary_commands();

            let rep = |t: Option<ares_compiler::types::TypeId>| t.map_or(Value::Null, |t| json!(c.types[t].representation));
            let sym = |s: Option<SymbolId>| match s.map(|s| &c.tables[s]) {
                None => Value::Null,
                Some(Symbol::Script { trigger, name, parameters, returns, .. }) => json!({
                    "$sym": "ServerScriptSymbol", "trigger": trigger.get().identifier, "name": name,
                    "parameters": c.types[*parameters].representation, "returns": c.types[*returns].representation,
                }),
                Some(Symbol::Basic { name, ty, is_protected }) => {
                    json!({ "$sym": "BasicSymbol", "name": name, "type": c.types[*ty].representation, "isProtected": is_protected })
                }
                Some(Symbol::LocalVariable { name, ty }) => json!({ "$sym": "LocalVariableSymbol", "name": name, "type": c.types[*ty].representation }),
                Some(other) => panic!("unexpected {other:?}"),
            };
            let scripts: Vec<Value> = files
                .iter()
                .flat_map(|f| &f.scripts)
                .map(|s| {
                    json!({
                        "triggerType": s.trigger_type.map_or("undefined", |t| t.get().identifier),
                        "parameterType": rep(s.parameter_type),
                        "returnType": rep(s.return_type),
                        "symbol": s.symbol.map_or(json!("undefined"), |x| sym(Some(x))),
                        "subjectReference": sym(s.subject_reference),
                        "parameters": s.parameters.iter().flatten().map(|p| sym(p.symbol)).collect::<Vec<_>>(),
                    })
                })
                .collect();
            let buckets: Vec<Value> = c
                .tables
                .buckets(SymbolTables::ROOT)
                .filter_map(|(ty, entries)| match ty {
                    SymbolType::ServerScript(t) => Some(json!([t.get().identifier, entries.map(|(k, _)| k).collect::<Vec<_>>()])),
                    _ => None,
                })
                .collect();
            let diags: Vec<Value> =
                diagnostics.list.iter().map(|d| json!([d.kind, d.source.name, d.source.line, d.source.column, d.formatted()])).collect();
            json!({ "parsed": parsed, "scripts": scripts, "buckets": buckets, "diagnostics": diags })
        })
        .collect();
    let mut failures = Vec::new();
    check_items(|i| format!("case #{i}"), &mut expected, actual, &mut failures);
    settle("registration corpus", &path, &expected, failures);
}

/// Type-checking corpus (`tests/typecheck/`). Per case the analyze stage
/// (registration, secondary commands, type checking) gives the typed AST per
/// parsed file (hash of the `ast-typed.json` shape) and the diagnostics with
/// formatted messages, or a fatal error. `TYPECHECK_SHOW=<case>` writes the
/// typed ASTs for a case to the temp dir.
#[test]
fn typecheck_corpus_matches() {
    use indexmap::IndexMap;
    use ares_compiler::runescript::compiler::{load_special_symbols, ServerScriptCompiler};
    use ares_compiler::runescript::type_info::CompilerTypeInfo;

    let inputs = read_json(&corpus("typecheck", "inputs.json"));
    let path = corpus("typecheck", "expected.json");
    let mut expected = read_json(&path);
    let actual: Vec<Value> = inputs
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, input)| {
            let symbols: IndexMap<String, CompilerTypeInfo> = serde_json::from_value(input["symbols"].clone()).unwrap();
            let mut mapper = Default::default();
            let pointers = load_special_symbols(&symbols["command"], &symbols["runescript"], &mut mapper, true).unwrap();
            let features = serde_json::from_value(input["features"].clone()).unwrap();
            let mut c = ServerScriptCompiler::new(symbols, mapper, pointers, features).unwrap();
            c.load_symbols().unwrap();
            let mut files = Vec::new();
            for f in input["files"].as_array().unwrap() {
                files.extend(ares_compiler::parser::parse_script_file(f["text"].as_str().unwrap(), f["name"].as_str().unwrap()).unwrap().node);
            }
            let mut diagnostics = Default::default();
            ares_compiler::semantics::registration::register_scripts(&mut c, &mut files, &mut diagnostics);
            c.register_secondary_commands();
            match ares_compiler::semantics::typecheck::type_check(&mut c, &mut files, &mut diagnostics) {
                Err(_) => json!({ "fatal": true }),
                Ok(()) => {
                    let dump = ares_compiler::ast::dump::Dump { types: &c.types, tables: &c.tables };
                    let trees: Vec<Value> = files.iter().map(|f| dump.script_file(f)).collect();
                    if std::env::var("TYPECHECK_SHOW").is_ok_and(|v| v == i.to_string()) {
                        let path = std::env::temp_dir().join("typecheck_show.json");
                        fs::write(&path, serde_json::to_string_pretty(&trees).unwrap()).unwrap();
                        println!("case #{i} written to {}", path.display());
                    }
                    let diags: Vec<Value> =
                        diagnostics.list.iter().map(|d| json!([d.kind, d.source.name, d.source.line, d.source.column, d.formatted()])).collect();
                    json!({ "files": trees.iter().map(|t| fnv1a64(&canonical_json(t))).collect::<Vec<_>>(), "diagnostics": diags })
                }
            }
        })
        .collect();
    let mut failures = Vec::new();
    check_items(|i| format!("case #{i}"), &mut expected, actual, &mut failures);
    settle("typecheck corpus", &path, &expected, failures);
}

/// Runs the pipeline on one `tests/pipeline` (or `tests/pointers`) case,
/// returning the per-case summary JSON. `PIPELINE_SHOW=<case>` writes that
/// case's IR to the temp dir.
fn pipeline_case(i: usize, input: &Value) -> Value {
    use indexmap::IndexMap;
    use ares_compiler::runescript::compiler::{load_special_symbols, ServerScriptCompiler};
    use ares_compiler::runescript::type_info::CompilerTypeInfo;

    let diag_json = |d: &ares_compiler::diagnostics::Diagnostics| -> Value {
        d.list.iter().map(|d| json!([d.kind, d.source.name, d.source.line, d.source.column, d.formatted()])).collect()
    };
    let symbols: IndexMap<String, CompilerTypeInfo> = serde_json::from_value(input["symbols"].clone()).unwrap();
    let mut mapper = Default::default();
    let pointers = load_special_symbols(&symbols["command"], &symbols["runescript"], &mut mapper, true).unwrap();
    let features = serde_json::from_value(input["features"].clone()).unwrap();
    let mut c = ServerScriptCompiler::new(symbols, mapper, pointers, features).unwrap();
    c.load_symbols().unwrap();
    let mut files = Vec::new();
    for f in input["files"].as_array().unwrap() {
        match ares_compiler::parser::parse_script_file(f["text"].as_str().unwrap(), f["name"].as_str().unwrap()).unwrap().node {
            Some(n) => files.push(n),
            None => return json!({ "stopped": "parse" }),
        }
    }
    let mut diagnostics = Default::default();
    ares_compiler::semantics::registration::register_scripts(&mut c, &mut files, &mut diagnostics);
    c.register_secondary_commands();
    if ares_compiler::semantics::typecheck::type_check(&mut c, &mut files, &mut diagnostics).is_err() {
        return json!({ "fatal": true });
    }
    if diagnostics.has_errors() {
        return json!({ "stopped": "analyze", "analyzeDiagnostics": diag_json(&diagnostics) });
    }
    let mut codegen = Default::default();
    let Ok(scripts) = ares_compiler::codegen::generate(&c, &files, &mut codegen) else { return json!({ "fatal": true }) };
    let ir: Vec<Value> = scripts.iter().map(|s| ares_compiler::codegen::ir::dump(s, &c.types, &c.tables)).collect();
    if std::env::var("PIPELINE_SHOW").is_ok_and(|v| v == i.to_string()) {
        let path = std::env::temp_dir().join("pipeline_show.json");
        fs::write(&path, serde_json::to_string_pretty(&ir).unwrap()).unwrap();
        println!("case #{i} written to {}", path.display());
    }
    let mut result = json!({ "ir": ir.iter().map(|s| fnv1a64(&canonical_json(s))).collect::<Vec<_>>(), "codegenDiagnostics": diag_json(&codegen) });
    let obj = result.as_object_mut().unwrap();
    if codegen.has_errors() {
        obj.insert("stopped".into(), json!("codegen"));
        return result;
    }
    if c.command_pointers.is_empty() {
        obj.insert("stopped".into(), json!("pointers-empty"));
        return result;
    }
    let mut pointer_diagnostics = Default::default();
    let Ok(ok) = ares_compiler::codegen::pointer::check_pointers(&c, &scripts, &mut pointer_diagnostics) else { return json!({ "fatal": true }) };
    let pd: Vec<Value> = pointer_diagnostics.list.iter().map(|d| json!([d.kind, d.source.name, d.source.line, d.source.column])).collect();
    obj.insert("pointerDiagnostics".into(), json!(pd));
    if !ok {
        obj.insert("stopped".into(), json!("pointers"));
        return result;
    }
    let mut written = Vec::new();
    for s in &scripts {
        let Ok(id) = c.mapper.get(&c.tables, s.symbol) else { return json!({ "fatal": true }) };
        let Ok(bytes) = ares_compiler::writer::binary::write_script(&c, s) else { return json!({ "fatal": true }) };
        written.push((id, bytes));
    }
    let Ok(packs) = ares_compiler::writer::pack::pack(&written) else { return json!({ "fatal": true }) };
    let h = |b: &[u8]| fnv1a64(&b.iter().map(|&x| x as char).collect::<String>());
    let bin: serde_json::Map<String, Value> = {
        let mut names: Vec<&String> = packs.bin.keys().collect();
        names.sort();
        names.into_iter().map(|n| (n.clone(), json!(h(&packs.bin[n])))).collect()
    };
    obj.insert("bytes".into(), json!({ "dat": h(&packs.script_dat), "idx": h(&packs.script_idx), "js5": h(&packs.js5), "bin": bin }));
    result
}

fn pipeline_corpus(dir: &str) {
    let inputs = read_json(&corpus(dir, "inputs.json"));
    let path = corpus(dir, "expected.json");
    let mut expected = read_json(&path);
    // A filtered run only reports; it never blesses a partial snapshot.
    let show = std::env::var("PIPELINE_SHOW").ok();
    let actual: Vec<Value> = inputs.as_array().unwrap().iter().enumerate().map(|(i, input)| pipeline_case(i, input)).collect();
    let mut failures = Vec::new();
    check_items(|i| format!("case #{i}"), &mut expected, actual, &mut failures);
    if show.is_some() {
        report(dir, failures);
    } else {
        settle(dir, &path, &expected, failures);
    }
}

/// Pipeline corpus (`tests/pipeline/`): mostly type-correct programs through every stage.
#[test]
fn pipeline_corpus_matches() {
    pipeline_corpus("pipeline");
}

/// Pointer-checking corpus (`tests/pointers/`): every stage up to and
/// including pointer checking, pointer diagnostics by type and position.
#[test]
fn pointer_corpus_matches() {
    pipeline_corpus("pointers");
}
