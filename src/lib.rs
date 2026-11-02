//! `ares-compiler`: the LostCityRS RuneScript compiler.
//!
//! Entry point: [`compile_server_script`] with a [`Config`]; [`compile`] does
//! the same without writing packs; [`analyze`] parses and type checks in-memory
//! sources for editor tooling.
//!
//! | module | purpose |
//! |---|---|
//! | `lexer` | tokens |
//! | `ast`, `parser` | syntax tree and parser |
//! | `preprocess` | macro expansion |
//! | `types`, `symbol`, `trigger`, `pointer`, `features`, `diagnostics` | compiler data model |
//! | `semantics::{registration,typecheck}` | script registration and type checking |
//! | `codegen` (+ `codegen::pointer`) | instruction generation and pointer checking |
//! | `writer` | binary script and pack writers |
//! | `compiler` | source discovery and the stage pipeline |
//! | `runescript::*` | server compiler setup, symbol loaders, command handlers, triggers |

pub use compiler::{analyze_parsed, recheck_file, same_headers};
pub use runescript::compiler::{analyze, compile, compile_server_script, Config, WriterConfig};

pub mod ast;
pub mod codegen;
pub mod compiler;
pub mod diagnostics;
pub mod features;
pub mod lexer;
pub mod parser;
pub mod pointer;
pub mod preprocess;
pub mod runescript;
pub mod semantics;
pub mod symbol;
pub mod trigger;
pub mod types;
pub mod writer;

/// An error that aborts the whole compilation (no diagnostics, no output).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FatalError(pub String);

impl std::fmt::Display for FatalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FatalError {}

impl From<types::TypeError> for FatalError {
    fn from(e: types::TypeError) -> Self {
        FatalError(e.0)
    }
}
