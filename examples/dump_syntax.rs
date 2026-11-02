//! Prints the word lists the VS Code extension's grammar needs, as JSON:
//! `cargo run --release --example dump_syntax > ../vscode-runescript/data/syntax.json`.

use ares_compiler::runescript::triggers::SERVER_TRIGGERS;
use serde_json::json;

/// Type names registered by hand in `runescript::compiler::Compiler::setup`
/// (not in `ares_shared::var_type`). Keep in step with that function.
const SETUP_TYPES: &[&str] = &[
    "type", "proc", "label", "walktrigger", "queue", "timer", "softtimer", "param", "intparam", "varp",
    "varbit", "dbcolumn",
];

fn main() {
    let triggers: Vec<&str> =
        std::iter::once("command").chain(SERVER_TRIGGERS.iter().map(|t| t.identifier)).collect();
    let types: Vec<&str> = ares_shared::var_type::PRIMITIVE
        .iter()
        .chain(ares_shared::var_type::SCRIPT)
        .map(|(name, _)| *name)
        .chain(SETUP_TYPES.iter().copied())
        .collect();
    let keywords = ["if", "else", "while", "case", "default", "return", "calc"];
    let out = json!({ "triggers": triggers, "types": types, "keywords": keywords });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
