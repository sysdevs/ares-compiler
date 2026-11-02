//! `analyze` (in-memory parse + registration + typecheck for editor tooling)
//! agrees with `compile` on every fixture, and keeps going past syntax errors.

use std::fs;
use std::path::PathBuf;

use indexmap::IndexMap;
use ares_compiler::compiler::{SourceNames, Stage};
use ares_compiler::runescript::type_info::CompilerTypeInfo;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn symbols(case: &str) -> IndexMap<String, CompilerTypeInfo> {
    serde_json::from_str(&fs::read_to_string(fixtures().join(case).join("symbols.json")).unwrap()).unwrap()
}

/// A fixture's sources as (relative name, text), bytewise like the compiler's walk.
fn sources(case: &str) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = fs::read_dir(fixtures().join(case).join("scripts"))
        .unwrap()
        .map(|e| e.unwrap())
        .map(|e| (e.file_name().to_string_lossy().into_owned(), fs::read_to_string(e.path()).unwrap()))
        .collect();
    v.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    v
}

fn stage(diags: &[(Stage, ares_compiler::diagnostics::Diagnostics)], s: Stage) -> Option<String> {
    diags.iter().find(|(st, _)| *st == s).map(|(_, d)| format!("{d:?}"))
}

#[test]
fn analyze_matches_compile_diagnostics() {
    let cases: Vec<String> = {
        let mut v: Vec<String> =
            fs::read_dir(fixtures()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    };
    for case in cases {
        let config = ares_compiler::Config {
            source_paths: vec![fixtures().join(&case).join("scripts")],
            symbols: symbols(&case),
            source_names: SourceNames::Relative,
            ..Default::default()
        };
        let compiled = match ares_compiler::compile(&config) {
            Ok(o) => o,
            Err(_) => continue, // fatal cases have nothing to compare
        };
        let analysis = ares_compiler::analyze(&config, &sources(&case)).unwrap();
        assert_eq!(stage(&analysis.diagnostics, Stage::Parse), stage(&compiled.diagnostics, Stage::Parse), "{case}: parse");
        if compiled.stopped != Some(Stage::Parse) {
            assert_eq!(
                stage(&analysis.diagnostics, Stage::Analyze),
                stage(&compiled.diagnostics, Stage::Analyze),
                "{case}: analyze"
            );
        }
    }
}

#[test]
fn analyze_continues_past_syntax_errors() {
    let config = ares_compiler::Config { symbols: symbols("07-commands"), ..Default::default() };
    let mut srcs = sources("07-commands");
    srcs.push(("broken.rs2".into(), "[proc,broken]\nif (\n".into()));
    srcs.push(("typo.rs2".into(), "[proc,typo]\n$undefined = 1;\n".into()));
    let analysis = ares_compiler::analyze(&config, &srcs).unwrap();
    let parse = stage(&analysis.diagnostics, Stage::Parse).unwrap();
    let analyze = stage(&analysis.diagnostics, Stage::Analyze).unwrap();
    assert!(parse.contains("broken.rs2"), "{parse}");
    assert!(analyze.contains("typo.rs2"), "the file after the broken one is still type checked: {analyze}");
    assert_eq!(analysis.files.len(), srcs.len() - 1);
}

/// One file's Analyze diagnostics as `line:col: message`.
fn of_file(d: &ares_compiler::diagnostics::Diagnostics, name: &str) -> Vec<String> {
    d.list.iter().filter(|d| d.source.name == name).map(|d| format!("{}:{}: {}", d.source.line, d.source.column, d.formatted())).collect()
}

/// Re-checking any one file in place (`recheck_file`, the editor's per-edit
/// path) gives exactly the diagnostics a full analysis gives that file, also
/// after other files were re-checked before it.
#[test]
fn recheck_file_matches_full_analysis() {
    let mut cases: Vec<String> = fs::read_dir(fixtures()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    cases.sort();
    let mut checked = 0;
    for case in cases {
        let config = ares_compiler::Config { symbols: symbols(&case), ..Default::default() };
        let srcs = sources(&case);
        if srcs.iter().any(|(n, _)| n.ends_with(".macro")) {
            continue; // re-parses here are not macro-expanded; editors take the full path with macros
        }
        let Ok(mut full) = ares_compiler::analyze(&config, &srcs) else { continue };
        let analyze = full.diagnostics.iter().find(|(s, _)| *s == Stage::Analyze).map(|(_, d)| d.clone()).unwrap();
        for i in 0..full.files.len() {
            let Some(name) = full.files[i].scripts.first().map(|s| s.source.name.clone()) else { continue };
            let text = &srcs.iter().find(|(n, _)| *n == name).unwrap().1;
            let mut fresh = ares_compiler::parser::parse_script_file(text, &name).unwrap().node.unwrap();
            let got = ares_compiler::recheck_file(&mut full.compiler, &mut fresh, &full.files[i]).unwrap();
            assert_eq!(of_file(&got, &name), of_file(&analyze, &name), "{case}/{name}");
            full.files[i] = fresh;
            checked += 1;
        }
    }
    assert!(checked >= 15, "only {checked} files re-checked");
}
