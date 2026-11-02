//! rustc-style rendering of compile errors: each `tests/render/<case>/` is
//! compiled together with `tests/render/commands/` (or alone, with its own
//! `symbols.json`) and the rendered diagnostics must equal its `expected.txt`
//! (`BLESS=1` rewrites them).

use std::fs;
use std::path::PathBuf;

use ares_compiler::compiler::SourceNames;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/render")
}

#[test]
fn rendered_diagnostics_match() {
    let symbols: indexmap::IndexMap<String, ares_compiler::runescript::type_info::CompilerTypeInfo> = serde_json::from_str(&fs::read_to_string(root().join("symbols.json")).unwrap()).unwrap();
    let mut cases: Vec<String> = fs::read_dir(root()).unwrap().map(|e| e.unwrap()).filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    cases.retain(|c| c != "commands");
    cases.sort();
    let mut failures = Vec::new();
    for case in cases {
        // A case with its own `symbols.json` (pointer rules) declares its own commands.
        let own = root().join(&case).join("symbols.json");
        let (source_paths, symbols) = match fs::read_to_string(&own) {
            Ok(text) => (vec![root().join(&case)], serde_json::from_str(&text).unwrap()),
            Err(_) => (vec![root().join("commands"), root().join(&case)], symbols.clone()),
        };
        let config = ares_compiler::Config {
            source_paths,
            symbols,
            source_names: SourceNames::Relative,
            features: ares_compiler::features::StrictFeatureLevel { macros: true, ..Default::default() },
            ..Default::default()
        };
        let actual = ares_compiler::compile(&config).unwrap().render_diagnostics();
        let path = root().join(&case).join("expected.txt");
        let expected = fs::read_to_string(&path).unwrap_or_default().replace("
", "
");
        if std::env::var("BLESS").is_ok() {
            if actual != expected {
                fs::write(&path, &actual).unwrap();
            }
        } else if actual != expected {
            failures.push(format!("{case}: rendered diagnostics differ
--- expected
{expected}
--- actual
{actual}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("

"));
}
