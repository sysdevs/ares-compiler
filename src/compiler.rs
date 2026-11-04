//! Source discovery, the stage pipeline (parse, analyze, codegen, pointer
//! check, write) and the core type/trigger registration.
//!
//! - By default the source name embedded in every script is the entry joined
//!   under the absolute, normalised source path. [`SourceNames::Relative`]
//!   instead names files by their normalised path relative to the source path
//!   (as if compiled from inside it), for machine-independent output.
//! - Directory entries are walked in bytewise name order (`readdir` order on
//!   Unix; pinned, NTFS differs).
//! - A stage with errors stops compilation, so nothing is written. An empty
//!   command pointer table stops compilation *without* errors, and the writer
//!   still closes: empty packs.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf, MAIN_SEPARATOR};

use crate::ast::ScriptFile;
use crate::codegen;
use crate::diagnostics::render::{render_all, SourceFile};
use crate::diagnostics::{DiagnosticType, Diagnostics};
use crate::preprocess::{expand_source_with_map, parse_macro_file, MacroDiagnostic, MacroExpansionSpan, MacroRegistry};
use crate::runescript::compiler::ServerScriptCompiler;
use crate::semantics::{registration, typecheck};
use crate::types::{TypeKind, TypeManager, ANY, ERROR, PRIMITIVES};
use crate::writer::{binary, pack};
use crate::FatalError;

/// Registers the core types and default type checkers.
pub fn register_core(types: &mut TypeManager) -> Result<(), FatalError> {
    for t in PRIMITIVES {
        types.register(t)?;
    }
    types.register(ANY)?;
    types.register_as("type", ANY)?;
    add_default_type_checkers(types);
    Ok(())
}

/// A pair is compatible if any checker accepts it, so only the set matters,
/// but the order is kept.
fn add_default_type_checkers(types: &mut TypeManager) {
    // Anything is assignable to `any`.
    types.add_type_checker(Box::new(|_, l, _| l == ANY));
    // `error` on either side is compatible, to stop error propagation.
    types.add_type_checker(Box::new(|_, l, r| l == ERROR || r == ERROR));
    // Identity.
    types.add_type_checker(Box::new(|_, l, r| l == r));
    // Script types: same trigger, compatible parameter and return types.
    types.add_type_checker(Box::new(|m, l, r| match (&m[l].kind, &m[r].kind) {
        (
            TypeKind::Script { trigger: lt, parameter_type: lp, return_type: lr },
            TypeKind::Script { trigger: rt, parameter_type: rp, return_type: rr },
        ) => lt == rt && m.check(*lp, *rp) && m.check(*lr, *rr),
        _ => false,
    }));
    // Wrapped types: same kind, compatible inner types.
    types.add_type_checker(Box::new(|m, l, r| {
        let (lk, rk) = (&m[l].kind, &m[r].kind);
        match (lk.inner(), rk.inner()) {
            (Some(a), Some(b)) => std::mem::discriminant(lk) == std::mem::discriminant(rk) && m.check(a, b),
            _ => false,
        }
    }));
    // Tuples: same arity, pairwise compatible.
    types.add_type_checker(Box::new(|m, l, r| match (&m[l].kind, &m[r].kind) {
        (TypeKind::Tuple(a), TypeKind::Tuple(b)) => a.len() == b.len() && a.iter().zip(b).all(|(&x, &y)| m.check(x, y)),
        _ => false,
    }));
    // Fallback: same representation string.
    types.add_type_checker(Box::new(|m, l, r| m[l].representation == m[r].representation));
}

/// How source files are named in the output (`RuneScript.sourceName`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SourceNames {
    /// Absolute, normalised OS paths.
    #[default]
    Absolute,
    /// The normalised path relative to the source path.
    Relative,
    /// `<prefix>/<path relative to the source path>` joined with `/` and not
    /// normalised (e.g. a `../content/scripts` prefix).
    Prefixed(&'static str),
}

/// Which script language a compile accepts: server `.rs2` or client `.cs2`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Dialect {
    #[default]
    Server,
    Client,
}

impl Dialect {
    /// The source file extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Dialect::Server => "rs2",
            Dialect::Client => "cs2",
        }
    }
}

/// The pipeline stage that stopped compilation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Parse,
    Analyze,
    Codegen,
    Pointers,
}

/// What a compilation produced: every stage's diagnostics, the stage that
/// stopped it (errors), and the packs that were written.
#[derive(Debug)]
pub struct Output {
    pub diagnostics: Vec<(Stage, Diagnostics)>,
    /// `Some` when a stage reported errors; nothing is written then.
    pub stopped: Option<Stage>,
    /// `None` when compilation stopped on errors; empty packs when the command
    /// pointer table is empty.
    pub packs: Option<pack::Packs>,
    pub scripts_written: usize,
    /// The script sources (`.rs2`/`.cs2`) as read, for rendering diagnostics.
    pub sources: Vec<SourceFile>,
}

impl Output {
    /// Every diagnostic rustc-style (source snippets, notes), then the
    /// `could not compile` line when any is an error.
    pub fn render_diagnostics(&self) -> String {
        render_all(self.diagnostics.iter().flat_map(|(_, d)| &d.list), &self.sources)
    }
}

/// Lexical `.`/`..` folding with the OS separator (`.` for an empty result).
pub fn normalize_lexically(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                } else if !out.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() { PathBuf::from(".") } else { out }
}

/// `p` made absolute against the current directory, then [`normalize_lexically`].
pub fn resolve(p: &Path) -> PathBuf {
    let abs = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().expect("cwd").join(p) };
    normalize_lexically(&abs)
}

/// Moves the pending macro errors into `parse`.
fn report_macro_errors(parse: &mut Diagnostics, diags: &mut Vec<MacroDiagnostic>) {
    for d in diags.drain(..) {
        let loc = crate::ast::Loc { name: d.file, line: d.line as i32, column: d.column as i32, end_line: d.line as i32, end_column: d.column as i32 };
        parse.error(&loc, &d.message, vec![]);
    }
}

fn name_string(p: &Path) -> String {
    let s = p.to_string_lossy().into_owned();
    if MAIN_SEPARATOR == '\\' { s.replace('/', "\\") } else { s }
}

/// Files under `dir` (entries in bytewise order, directories
/// inline) as (real path, source name).
fn walk(dir: &Path, name: &Path, out: &mut Vec<(PathBuf, String)>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by(|a, b| a.file_name().to_string_lossy().as_bytes().cmp(b.file_name().to_string_lossy().as_bytes()));
    for e in entries {
        let child_name = normalize_lexically(&name.join(e.file_name()));
        if e.file_type()?.is_dir() {
            walk(&e.path(), &child_name, out)?;
        } else {
            out.push((e.path(), name_string(&child_name)));
        }
    }
    Ok(())
}

/// Errors that abort a compilation outright.
#[derive(Debug)]
pub enum CompileError {
    Fatal(FatalError),
    Io(std::io::Error),
}

impl From<FatalError> for CompileError {
    fn from(e: FatalError) -> Self {
        CompileError::Fatal(e)
    }
}

impl From<std::io::Error> for CompileError {
    fn from(e: std::io::Error) -> Self {
        CompileError::Io(e)
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::Fatal(e) => write!(f, "{e}"),
            CompileError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CompileError {}

/// Expands macros and parses `sources` (`(name, text)`, in order): `.macro` files
/// define macros (when enabled), `.rs2` files become trees. Files with syntax
/// errors are reported in `parse` and left out.
fn parse_sources(c: &ServerScriptCompiler, sources: &[(String, String)], parse: &mut Diagnostics, expansions: &mut HashMap<String, Vec<MacroExpansionSpan>>) -> Result<Vec<ScriptFile>, CompileError> {
    let mut registry = MacroRegistry::default();
    let mut macro_diags = Vec::new();
    if c.features.macros {
        for (name, text) in sources {
            if name.ends_with(".macro") {
                parse_macro_file(name, text, &mut registry, &mut macro_diags);
            }
        }
    }
    let mut files = Vec::new();
    let ext = format!(".{}", c.dialect.extension());
    for (name, raw) in sources {
        if !name.ends_with(&ext) {
            continue;
        }
        let expanded;
        let text = if c.features.macros {
            let spans;
            (expanded, spans) = expand_source_with_map(raw, &registry, &mut macro_diags, name);
            if !spans.is_empty() {
                expansions.insert(name.clone(), spans);
            }
            &expanded
        } else {
            raw
        };
        let parsed = crate::parser::parse_script_file(text, name)?;
        report_macro_errors(parse, &mut macro_diags);
        for e in parsed.errors {
            let loc = crate::ast::Loc { name: name.clone(), line: e.line, column: e.column, end_line: e.line, end_column: e.column };
            parse.report(DiagnosticType::SyntaxError, &loc, &e.message, vec![]);
        }
        files.extend(parsed.node);
    }
    report_macro_errors(parse, &mut macro_diags);
    Ok(files)
}

/// What [`analyze`] produced: the compiler after typecheck (symbol tables and
/// types), the typed tree of every file that parsed, and the Parse and Analyze
/// diagnostics.
pub struct Analysis {
    pub compiler: ServerScriptCompiler,
    pub files: Vec<ScriptFile>,
    pub diagnostics: Vec<(Stage, Diagnostics)>,
}

/// Parse, registration and typecheck over in-memory `(name, text)` sources, for
/// editor tooling. Unlike [`run`], syntax errors in one file do not stop the
/// others from being analyzed; codegen, pointer checks and writing are skipped.
pub fn analyze(c: ServerScriptCompiler, sources: &[(String, String)]) -> Result<Analysis, CompileError> {
    let mut parse = Diagnostics::default();
    let files = parse_sources(&c, sources, &mut parse, &mut HashMap::new())?;
    let mut a = analyze_parsed(c, files)?;
    a.diagnostics.insert(0, (Stage::Parse, parse));
    Ok(a)
}

/// [`analyze`] over files the caller parsed (no macro expansion), so an editor
/// can keep parses of unchanged files. Only Analyze diagnostics come back.
pub fn analyze_parsed(mut c: ServerScriptCompiler, mut files: Vec<ScriptFile>) -> Result<Analysis, CompileError> {
    c.load_symbols()?;
    let mut analyze = Diagnostics::default();
    registration::register_scripts(&mut c, &mut files, &mut analyze);
    c.register_secondary_commands();
    typecheck::type_check(&mut c, &mut files, &mut analyze)?;
    Ok(Analysis { compiler: c, files, diagnostics: vec![(Stage::Analyze, analyze)] })
}

/// Whether two parses of a file declare the same scripts (trigger, name,
/// parameter and return types, in order), so [`recheck_file`] may replace one with the other.
pub fn same_headers(a: &ScriptFile, b: &ScriptFile) -> bool {
    let header = |f: &ScriptFile| -> Vec<(String, String, bool, Vec<String>, Vec<String>)> {
        f.scripts
            .iter()
            .map(|s| {
                let params = s.parameters.iter().flatten().map(|p| p.type_token.text.clone()).collect();
                let returns = s.return_tokens.iter().flatten().map(|t| t.text.clone()).collect();
                (s.trigger.text.clone(), s.name.text.clone(), s.is_star, params, returns)
            })
            .collect()
    };
    header(a) == header(b)
}

/// Re-analyzes one edited file inside a finished [`Analysis`], whose `previous`
/// parse it replaces: registration reuses the script symbols `previous` got, then
/// the file is type checked. Requires [`same_headers`]`(file, previous)`; other
/// files are untouched, since nothing they see changed. Returns the file's Analyze
/// diagnostics. The replaced parse's locals stay behind in the tables until the
/// next full [`analyze_parsed`].
pub fn recheck_file(c: &mut ServerScriptCompiler, file: &mut ScriptFile, previous: &ScriptFile) -> Result<Diagnostics, CompileError> {
    debug_assert!(same_headers(file, previous));
    let mut d = Diagnostics::default();
    registration::reregister_file(c, file, previous, &mut d);
    typecheck::type_check(c, std::slice::from_mut(file), &mut d)?;
    Ok(d)
}

/// Runs a set-up compiler over the `.rs2` files: load symbols, then the
/// stages, then write (in memory).
pub fn run(
    c: &mut ServerScriptCompiler,
    source_paths: &[PathBuf],
    exclude_paths: &[PathBuf],
    names: SourceNames,
) -> Result<Output, CompileError> {
    c.load_symbols()?;
    let mut output = Output { diagnostics: Vec::new(), stopped: None, packs: None, scripts_written: 0, sources: Vec::new() };
    let stop = |output: &mut Output, stage: Stage, d: Diagnostics| {
        let errors = d.has_errors();
        output.diagnostics.push((stage, d));
        if errors {
            output.stopped = Some(stage);
        }
        errors
    };

    // parse
    let mut files_by_source = Vec::new();
    for sp in source_paths {
        let root = resolve(sp);
        let base = match names {
            SourceNames::Absolute => root.clone(),
            SourceNames::Relative | SourceNames::Prefixed(_) => PathBuf::from("."),
        };
        let mut files = Vec::new();
        walk(&root, &base, &mut files)?;
        if let SourceNames::Prefixed(prefix) = names {
            for (_, name) in &mut files {
                *name = format!("{prefix}/{}", name.replace('\\', "/"));
            }
        }
        files_by_source.push(files);
    }
    // Lossy UTF-8; a leading BOM is dropped.
    let read = |p: &Path| -> Result<String, CompileError> {
        let bytes = std::fs::read(p)?;
        Ok(String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).into_owned())
    };
    let mut sources = Vec::new();
    for (path, name) in files_by_source.iter().flatten() {
        if name.ends_with(&format!(".{}", c.dialect.extension())) || (c.features.macros && name.ends_with(".macro")) {
            sources.push((name.clone(), read(path)?));
        }
    }
    let mut parse = Diagnostics::default();
    let mut expansions = HashMap::new();
    let mut files = parse_sources(c, &sources, &mut parse, &mut expansions)?;
    output.sources = sources
        .into_iter()
        .filter(|(name, _)| name.ends_with(&format!(".{}", c.dialect.extension())))
        .map(|(name, text)| SourceFile { expansions: expansions.remove(&name).unwrap_or_default(), name, text })
        .collect();
    if stop(&mut output, Stage::Parse, parse) {
        return Ok(output);
    }

    // analyze
    let mut analyze = Diagnostics::default();
    registration::register_scripts(c, &mut files, &mut analyze);
    c.register_secondary_commands();
    typecheck::type_check(c, &mut files, &mut analyze)?;
    if stop(&mut output, Stage::Analyze, analyze) {
        return Ok(output);
    }

    // codegen
    let mut generation = Diagnostics::default();
    let scripts = codegen::generate(c, &files, &mut generation)?;
    if stop(&mut output, Stage::Codegen, generation) {
        return Ok(output);
    }

    // pointer check: an empty table skips writing without errors.
    let mut pointers = Diagnostics::default();
    let ok = codegen::pointer::check_pointers(c, &scripts, &mut pointers)?;
    let had_table = !c.command_pointers.is_empty();
    if had_table && stop(&mut output, Stage::Pointers, pointers) {
        return Ok(output);
    }
    if !ok {
        output.packs = Some(pack::pack(&[])?);
        return Ok(output);
    }

    // write: exclusion matches the file's real path (source names may be relative).
    let excluded: Vec<PathBuf> = exclude_paths.iter().map(|p| resolve(p)).collect();
    let mut real_paths: HashMap<&str, &Path> = HashMap::new();
    for (path, name) in files_by_source.iter().flatten() {
        real_paths.entry(name.as_str()).or_insert(path.as_path());
    }
    let mut written = Vec::new();
    for s in &scripts {
        let source = real_paths.get(s.source_name.as_str()).map_or_else(|| resolve(Path::new(&s.source_name)), |p| normalize_lexically(p));
        if excluded.iter().any(|e| source == *e || source.starts_with(e)) {
            continue;
        }
        written.push((c.mapper.get(&c.tables, s.symbol)?, binary::write_script(c, s)?));
    }
    output.scripts_written = written.len();
    output.packs = Some(pack::pack(&written)?);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_path_semantics() {
        let sep = MAIN_SEPARATOR;
        assert_eq!(normalize_lexically(Path::new("./a.rs2")), PathBuf::from("a.rs2"));
        assert_eq!(normalize_lexically(Path::new(".")), PathBuf::from("."));
        assert_eq!(name_string(&normalize_lexically(Path::new("./sub/../x/y.rs2"))), format!("x{sep}y.rs2"));
        assert_eq!(normalize_lexically(Path::new("../a")), PathBuf::from("../a"));
        assert!(resolve(Path::new("a/../b")).ends_with("b"));
    }
}
