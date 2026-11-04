//! The server script compiler: setup (types, triggers, loaders, dynamic command
//! handlers) plus symbol loading. Registration order is load order, and load order
//! is the root table's `SymbolType` order, which decides `find_all` resolution.

use crate::compiler::Dialect;
use std::collections::HashMap;

use indexmap::IndexMap;

use crate::FatalError;
use crate::features::StrictFeatureLevel;
use crate::pointer::{PointerHolder, PointerType};
use crate::runescript::commands::{CommandHandler, HookTriggers};
use crate::runescript::loaders::{SymbolLoader, TypeSupplier};
use crate::runescript::symbol_mapper::SymbolMapper;
use crate::runescript::triggers;
use crate::runescript::type_info::CompilerTypeInfo;
use crate::runescript::types::{self as sv, SCRIPT_VAR_TYPES};
use crate::symbol::{Symbol, SymbolTables, SymbolType};
use crate::trigger::COMMAND;
use crate::types::{ANY, INT, NOTHING, TypeId, TypeManager, UNIT};

pub struct ServerScriptCompiler {
    pub types: TypeManager,
    pub tables: SymbolTables,
    pub mapper: SymbolMapper,
    pub features: StrictFeatureLevel,
    pub dialect: Dialect,
    /// Command name (`.name` for the secondary form) → pointer requirements.
    pub command_pointers: IndexMap<String, PointerHolder>,
    /// Command name → handler.
    pub dynamic_commands: HashMap<String, CommandHandler>,
    symbols: IndexMap<String, CompilerTypeInfo>,
    loaders: Vec<SymbolLoader>,
}

impl ServerScriptCompiler {
    /// Creates the compiler and registers its types, triggers, loaders and handlers.
    pub fn new(
        symbols: IndexMap<String, CompilerTypeInfo>,
        mapper: SymbolMapper,
        command_pointers: IndexMap<String, PointerHolder>,
        features: StrictFeatureLevel,
        dialect: Dialect,
    ) -> Result<Self, FatalError> {
        let mut c = Self {
            types: TypeManager::new(),
            tables: SymbolTables::default(),
            mapper,
            features,
            dialect,
            command_pointers,
            dynamic_commands: HashMap::new(),
            symbols,
            loaders: Vec::new(),
        };
        crate::compiler::register_core(&mut c.types)?;
        c.setup()?;
        Ok(c)
    }

    /// One table of the compiler's `symbols.json`.
    pub fn symbol_info(&self, name: &str) -> Option<&CompilerTypeInfo> {
        self.symbols.get(name)
    }

    /// Runs every loader into the root table.
    pub fn load_symbols(&mut self) -> Result<(), FatalError> {
        for loader in &self.loaders {
            loader.load(&mut self.tables, &mut self.types, &mut self.mapper)?;
        }
        Ok(())
    }

    /// Runs right after script registration: for
    /// every `.name` pointer entry whose base command is registered, add a `.name`
    /// command alias with the base signature unless one exists.
    pub fn register_secondary_commands(&mut self) {
        let command = SymbolType::ServerScript(COMMAND);
        for name in self.command_pointers.keys() {
            let Some(base) = name.strip_prefix('.').filter(|b| !b.is_empty()) else {
                continue;
            };
            let Some(base) = self.tables.find(SymbolTables::ROOT, command, base) else {
                continue;
            };
            let Symbol::Script {
                parameters,
                returns,
                ..
            } = self.tables[base]
            else {
                continue;
            };
            if self
                .tables
                .find(SymbolTables::ROOT, command, name)
                .is_some()
            {
                continue;
            }
            let alias = self.tables.add(Symbol::Script {
                trigger: COMMAND,
                name: name.clone(),
                parameters,
                returns,
            });
            self.tables.insert(SymbolTables::ROOT, command, alias);
        }
    }

    fn setup(&mut self) -> Result<(), FatalError> {
        let f = self.features;
        self.register_script_var_types()?;
        self.types.change_options("long", |o| {
            o.allow_declaration = false;
            o.allow_parameter = true;
        })?;
        if f.procs {
            self.register_script("proc", triggers::PROC, UNIT, UNIT)?;
        }
        self.register_script("label", triggers::LABEL, UNIT, NOTHING)?;
        // Allow assignment of 'namedobj' to 'obj'.
        self.types
            .add_type_checker(Box::new(|_, l, r| l == sv::OBJ && r == sv::NAMEDOBJ));
        if let Some(info) = self.symbols.get("constant") {
            self.loaders.push(SymbolLoader::Constant(info.clone()));
        }
        self.register_script("walktrigger", triggers::WALKTRIGGER, ANY, NOTHING)?;
        let queue = self.register_script("queue", triggers::QUEUE, ANY, NOTHING)?;
        self.handlers(&[
            ("queue", CommandHandler::Queue(queue)),
            (".queue", CommandHandler::Queue(queue)),
        ])?;
        if f.queue_typed {
            self.handlers(&[
                ("queue*", CommandHandler::QueueVarArg(queue)),
                (".queue*", CommandHandler::QueueVarArg(queue)),
            ])?;
        }
        self.handlers(&[
            ("longqueue", CommandHandler::LongQueue(queue)),
            (".longqueue", CommandHandler::LongQueue(queue)),
        ])?;
        if f.queue_typed {
            self.handlers(&[
                ("longqueue*", CommandHandler::LongQueueVarArg(queue)),
                (".longqueue*", CommandHandler::LongQueueVarArg(queue)),
            ])?;
        }
        let timer = self.register_script("timer", triggers::TIMER, ANY, NOTHING)?;
        self.handlers(&[
            ("settimer", CommandHandler::Timer(timer)),
            (".settimer", CommandHandler::Timer(timer)),
        ])?;
        // One `param` of `any` shared by every param handler.
        let param_any = self.types.param(ANY);
        let param_handler = |ty| CommandHandler::Param { ty, param_any };
        self.handlers(&[
            ("lc_param", param_handler(Some(sv::LOC))),
            ("loc_param", param_handler(None)),
        ])?;
        self.sym_loader("loc", sv::LOC);
        self.handlers(&[
            ("nc_param", param_handler(Some(sv::NPC))),
            ("npc_param", param_handler(None)),
        ])?;
        self.sym_loader("npc", sv::NPC);
        self.handlers(&[
            ("oc_param", param_handler(Some(sv::OBJ))),
            ("obj_param", param_handler(None)),
        ])?;
        self.sym_loader("obj", sv::NAMEDOBJ);
        self.sym_loader("component", sv::COMPONENT);
        self.sym_loader("interface", sv::INTERFACE);
        self.sym_loader("overlayinterface", sv::OVERLAYINTERFACE);
        self.sym_loader("fontmetrics", sv::FONTMETRICS);
        self.sym_loader("category", sv::CATEGORY);
        self.sym_loader("hunt", sv::HUNT);
        self.sym_loader("inv", sv::INV);
        self.sym_loader("idk", sv::IDKIT);
        self.sym_loader("mesanim", sv::MESANIM);
        let param = self.types.param(ANY);
        self.types.register_as("param", param)?;
        let intparam = self.types.param(INT);
        self.types.register_as("intparam", intparam)?;
        self.loader("param", TypeSupplier::Param, false);
        self.sym_loader("seq", sv::SEQ);
        self.sym_loader("spotanim", sv::SPOTANIM);
        let varp = self.types.var_player(ANY);
        self.types.register_as("varp", varp)?;
        self.loader("varp", TypeSupplier::VarPlayer, true);
        self.loader("varn", TypeSupplier::VarNpc, false);
        self.loader("varo", TypeSupplier::VarObj, false);
        self.loader("vars", TypeSupplier::VarShared, false);
        self.sym_loader("stat", sv::STAT);
        self.sym_loader("locshape", sv::LOC_SHAPE);
        self.sym_loader("movespeed", sv::MOVESPEED);
        self.sym_loader("npc_mode", sv::NPC_MODE);
        self.sym_loader("npc_stat", sv::NPC_STAT);
        self.sym_loader("model", sv::MODEL);
        self.sym_loader("synth", sv::SYNTH);
        self.sym_loader("midi", sv::MIDI);
        self.sym_loader("jingle", sv::JINGLE);
        self.sym_loader("hitsplat", sv::HITSPLAT);

        let varbit = self.types.var_bit(ANY);
        self.types.register_as("varbit", varbit)?;
        self.loader("varbit", TypeSupplier::VarBit, true);
        // After varp and varbit: `%name` takes the first game var in load order.
        if self.dialect == Dialect::Client {
            let varc = self.types.var_client(ANY);
            self.types.register_as("varc", varc)?;
            self.loader("varc", TypeSupplier::VarClient, false);
            self.loaders.push(SymbolLoader::Event);
            for (event, kind) in [
                ("click", HookTriggers::None), ("hold", HookTriggers::None), ("release", HookTriggers::None),
                ("mouseover", HookTriggers::None), ("mouseleave", HookTriggers::None), ("drag", HookTriggers::None),
                ("vartransmit", HookTriggers::Var), ("timer", HookTriggers::None), ("op", HookTriggers::None),
                ("dragcomplete", HookTriggers::None), ("mouserepeat", HookTriggers::None), ("invtransmit", HookTriggers::Inv),
                ("stattransmit", HookTriggers::Stat), ("scrollwheel", HookTriggers::None), ("key", HookTriggers::None),
            ] {
                for prefix in ["cc", ".cc", "if", ".if"] {
                    self.handlers(&[(&format!("{prefix}_seton{event}"), CommandHandler::Hook(kind))])?;
                }
            }
        }
        self.handlers(&[
            ("weakqueue", CommandHandler::Queue(queue)),
            (".weakqueue", CommandHandler::Queue(queue)),
        ])?;
        if f.queue_typed {
            self.handlers(&[
                ("weakqueue*", CommandHandler::QueueVarArg(queue)),
                (".weakqueue*", CommandHandler::QueueVarArg(queue)),
            ])?;
        }
        self.handlers(&[
            ("strongqueue", CommandHandler::Queue(queue)),
            (".strongqueue", CommandHandler::Queue(queue)),
        ])?;
        if f.queue_typed {
            self.handlers(&[
                ("strongqueue*", CommandHandler::QueueVarArg(queue)),
                (".strongqueue*", CommandHandler::QueueVarArg(queue)),
            ])?;
        }
        if f.enums {
            self.handlers(&[("enum", CommandHandler::Enum)])?;
            self.sym_loader("enum", sv::ENUM);
        }
        if f.structs {
            self.handlers(&[("struct_param", param_handler(Some(sv::STRUCT)))])?;
            self.sym_loader("struct", sv::STRUCT);
        }
        let softtimer = self.register_script("softtimer", triggers::SOFTTIMER, ANY, NOTHING)?;
        self.handlers(&[
            ("softtimer", CommandHandler::Timer(softtimer)),
            (".softtimer", CommandHandler::Timer(softtimer)),
        ])?;
        if f.dbtables {
            let dbcolumn = self.types.db_column(ANY);
            self.types.register_as("dbcolumn", dbcolumn)?;
            self.handlers(&[
                ("db_find", CommandHandler::DbFind { with_count: false }),
                (
                    "db_find_refine",
                    CommandHandler::DbFind { with_count: false },
                ),
                (
                    "db_find_with_count",
                    CommandHandler::DbFind { with_count: true },
                ),
                (
                    "db_find_refine_with_count",
                    CommandHandler::DbFind { with_count: true },
                ),
                ("db_getfield", CommandHandler::DbGetField),
            ])?;
            self.loader("dbcolumn", TypeSupplier::DbColumn, false);
            self.sym_loader("dbrow", sv::DBROW);
            self.sym_loader("dbtable", sv::DBTABLE);
        }
        self.handlers(&[
            ("dump", CommandHandler::Dump),
            ("script", CommandHandler::Script),
        ])
    }

    /// Registers each pair, in order; a repeated name is an error.
    fn handlers(&mut self, handlers: &[(&str, CommandHandler)]) -> Result<(), FatalError> {
        for &(name, handler) in handlers {
            if self.dynamic_commands.contains_key(name) {
                return Err(FatalError(format!(
                    "A dynamic command handler with the name of '{name}' already exists."
                )));
            }
            self.dynamic_commands.insert(name.to_owned(), handler);
        }
        Ok(())
    }

    /// Registers the script var types, skipping feature-disabled ones.
    fn register_script_var_types(&mut self) -> Result<(), FatalError> {
        let f = self.features;
        for i in 0..SCRIPT_VAR_TYPES.len() {
            let t = TypeId(crate::types::FIRST_SCRIPT_VAR + i as u32);
            let disabled = (!f.enums && t == sv::ENUM)
                || (!f.structs && t == sv::STRUCT)
                || (!f.dbtables && (t == sv::DBROW || t == sv::DBTABLE));
            if !disabled {
                self.types.register(t)?;
            }
        }
        Ok(())
    }

    /// Registers a script type under `name` and returns it.
    fn register_script(
        &mut self,
        name: &str,
        trigger: crate::trigger::TriggerId,
        params: TypeId,
        returns: TypeId,
    ) -> Result<TypeId, FatalError> {
        let t = self.types.script(trigger, params, returns);
        self.types.register_as(name, t)?;
        Ok(t)
    }

    fn sym_loader(&mut self, name: &str, ty: TypeId) {
        self.loader(name, TypeSupplier::Fixed(ty), false);
    }

    /// Adds a loader only when `symbols[name]` exists.
    fn loader(&mut self, name: &str, supplier: TypeSupplier, protected: bool) {
        if let Some(info) = self.symbols.get(name) {
            self.loaders.push(SymbolLoader::Basic {
                info: info.clone(),
                supplier,
                protected,
            });
        }
    }
}

/// Loads command/script ids into
/// the mapper and, with `check_pointers`, the command pointer table (a command
/// with any `require`/`set`/`corrupt` gets an entry; its `.name` form gets one
/// when any of the `*2` lists is non-empty).
pub fn load_special_symbols(
    command_info: &CompilerTypeInfo,
    script_info: &CompilerTypeInfo,
    mapper: &mut SymbolMapper,
    check_pointers: bool,
) -> Result<IndexMap<String, PointerHolder>, FatalError> {
    let mut pointers = IndexMap::new();
    for (id, key, name) in command_info.ids()? {
        let c = command_info;
        if check_pointers
            && (text(&c.require, key).is_some()
                || text(&c.set, key).is_some()
                || text(&c.corrupt, key).is_some())
        {
            let conditional_set = c.conditional.get(key).copied().unwrap_or(false);
            let holder = |required, set, corrupted| -> Result<PointerHolder, FatalError> {
                Ok(PointerHolder {
                    required: parse_pointer_list(text(required, key))?,
                    set: parse_pointer_list(text(set, key))?,
                    conditional_set,
                    corrupted: parse_pointer_list(text(corrupted, key))?,
                })
            };
            let primary = holder(&c.require, &c.set, &c.corrupt)?;
            let secondary = holder(&c.require2, &c.set2, &c.corrupt2)?;
            // A repeated name keeps its position and takes the new value.
            pointers.insert(name.to_owned(), primary);
            if !secondary.required.is_empty()
                || !secondary.set.is_empty()
                || !secondary.corrupted.is_empty()
            {
                pointers.insert(format!(".{name}"), secondary);
            }
        }
        mapper.put_command(id, name);
    }
    for (id, _, name) in script_info.ids()? {
        mapper.put_script(id, name);
    }
    Ok(pointers)
}

/// A non-empty string entry (an empty string counts as absent).
fn text<'a>(m: &'a IndexMap<String, String>, k: &str) -> Option<&'a str> {
    m.get(k).map(String::as_str).filter(|s| !s.is_empty())
}

/// `none`/empty → empty; an unknown name is an error.
fn parse_pointer_list(text: Option<&str>) -> Result<indexmap::IndexSet<PointerType>, FatalError> {
    match text {
        None | Some("none") => Ok(Default::default()),
        Some(t) => t
            .split(',')
            .map(|n| {
                PointerType::for_name(n)
                    .ok_or_else(|| FatalError(format!("Invalid pointer name: {n}.")))
            })
            .collect(),
    }
}

/// Where `compile_server_script` writes (`writer.jag` or `writer.js5`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriterConfig {
    /// `<output>/script.dat` + `<output>/script.idx`.
    Jag { output: std::path::PathBuf },
    /// One js5 pack file.
    Js5 { output: std::path::PathBuf },
}

/// The compiler config; [`Default`] gives the stock defaults (sources
/// `../content/scripts`, jag output `./data/pack/server`, pointer checking on).
#[derive(Clone, Debug)]
pub struct Config {
    pub source_paths: Vec<std::path::PathBuf>,
    pub exclude_paths: Vec<std::path::PathBuf>,
    /// Must contain `command` and `runescript`.
    pub symbols: IndexMap<String, CompilerTypeInfo>,
    pub check_pointers: bool,
    pub features: StrictFeatureLevel,
    /// Server `.rs2` (default) or client `.cs2` scripts.
    pub dialect: Dialect,
    pub writer: WriterConfig,
    pub source_names: crate::compiler::SourceNames,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            source_paths: vec!["../content/scripts".into()],
            exclude_paths: Vec::new(),
            symbols: IndexMap::new(),
            check_pointers: true,
            features: StrictFeatureLevel::default(),
            dialect: Dialect::Server,
            writer: WriterConfig::Jag {
                output: "./data/pack/server".into(),
            },
            source_names: crate::compiler::SourceNames::Absolute,
        }
    }
}

/// A compiler set up from `config`: core tables checked, command/script ids and
/// pointer rules loaded. Single use: symbols are loaded when it runs.
pub fn new_compiler(config: &Config) -> Result<ServerScriptCompiler, FatalError> {
    let (Some(command), Some(runescript)) = (
        config.symbols.get("command"),
        config.symbols.get("runescript"),
    ) else {
        return Err(FatalError(
            "Core symbols missing from compiler. Please provide command and runescript symbols."
                .into(),
        ));
    };
    let mut mapper = SymbolMapper::default();
    let pointers = load_special_symbols(command, runescript, &mut mapper, config.check_pointers)?;
    ServerScriptCompiler::new(config.symbols.clone(), mapper, pointers, config.features, config.dialect)
}

/// Compiles in memory ([`compile_server_script`] without the file writes).
pub fn compile(config: &Config) -> Result<crate::compiler::Output, crate::compiler::CompileError> {
    let mut c = new_compiler(config)?;
    crate::compiler::run(
        &mut c,
        &config.source_paths,
        &config.exclude_paths,
        config.source_names,
    )
}

/// [`crate::compiler::analyze`] with a compiler set up from `config`
/// (`source_paths`, `exclude_paths` and `writer` are unused).
pub fn analyze(
    config: &Config,
    sources: &[(String, String)],
) -> Result<crate::compiler::Analysis, crate::compiler::CompileError> {
    crate::compiler::analyze(new_compiler(config)?, sources)
}

/// Compiles, then writes the configured pack.
/// Stage errors return the [`Output`](crate::compiler::Output) with nothing
/// written.
pub fn compile_server_script(
    config: &Config,
) -> Result<crate::compiler::Output, crate::compiler::CompileError> {
    let output = compile(config)?;
    if let Some(packs) = &output.packs {
        match &config.writer {
            WriterConfig::Jag { output: dir } => {
                let dir = crate::compiler::resolve(dir);
                std::fs::create_dir_all(&dir)?;
                std::fs::write(dir.join("script.dat"), &packs.script_dat)?;
                std::fs::write(dir.join("script.idx"), &packs.script_idx)?;
            }
            WriterConfig::Js5 { output: file } => {
                let file = crate::compiler::resolve(file);
                if let Some(parent) = file.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(file, &packs.js5)?;
            }
        }
    }
    Ok(output)
}
