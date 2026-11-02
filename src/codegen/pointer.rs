//! Pointer checking over a per-script control-flow graph.
//!
//! Pointer errors stop the compilation before writing, and pointer diagnostics
//! (errors and hints) are compared per file, so the order is pinned: the
//! control-flow graph keeps edge insertion order (it drives the BFS in
//! `find_edge_path`, hence which path and which hints are reported), and the
//! caches keep their recursion-order behaviour (a script reached while it is
//! still being calculated yields, and caches, an empty holder).
//!
//! Falling through into an empty block continues at the next non-empty block,
//! as branch targets do.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use indexmap::{IndexMap, IndexSet};

use super::ir::{Opcode, Operand, RuneScript};
use crate::ast::Loc;
use crate::diagnostics::{messages as msg, DiagnosticType, Diagnostics};
use crate::features::StrictFeatureLevel;
use crate::pointer::{PointerHolder, PointerType};
use crate::runescript::compiler::ServerScriptCompiler;
use crate::runescript::triggers;
use crate::symbol::{normalize_basic, Symbol, SymbolId};
use crate::types::{TypeId, TypeKind, TypeManager};
use crate::FatalError;

type R<T = ()> = Result<T, FatalError>;
type NodeId = usize;
/// (block index, instruction index) — instruction identity.
type InstrRef = (usize, usize);

const POINTERS: usize = 24;

fn fatal<T>(m: &str) -> R<T> {
    Err(FatalError(m.into()))
}

/// `ScriptCompiler.checkPointers`: `Ok(false)` (nothing gets written) when the
/// command pointer table is empty, else whether no error was reported.
pub fn check_pointers(c: &ServerScriptCompiler, scripts: &[RuneScript], diagnostics: &mut Diagnostics) -> R<bool> {
    if c.command_pointers.is_empty() {
        return Ok(false);
    }
    let overlays = c
        .symbol_info("overlayinterface")
        .map(|info| info.map.values().map(|n| normalize_basic(n)).collect())
        .unwrap_or_default();
    let mut checker = PointerChecker {
        c,
        scripts,
        d: diagnostics,
        features: c.features,
        overlay_interfaces: overlays,
        scripts_by_symbol: scripts.iter().enumerate().map(|(i, s)| (s.symbol, i)).collect(),
        graphs: HashMap::new(),
        pointers: HashMap::new(),
        analyses: HashMap::new(),
        jump_param_nodes: HashMap::new(),
        pending_analyses: HashSet::new(),
        pending_scripts: HashSet::new(),
    };
    for i in 0..scripts.len() {
        for &p in PointerType::ALL {
            checker.validate_pointer(i, p)?;
        }
    }
    Ok(!diagnostics.has_errors())
}

#[derive(Debug)]
enum NodeKind {
    /// The graph's entry node (no instruction).
    Start,
    Instruction(InstrRef),
    /// A conditional setter's pointers.
    Pointer(IndexSet<PointerType>),
}

#[derive(Debug)]
struct Node {
    kind: NodeKind,
    previous: Vec<NodeId>,
}

#[derive(Debug, Default)]
struct Graph {
    nodes: Vec<Node>,
    /// The `nodes` array `generate` returns (`graph[0]` is the start node).
    list: Vec<NodeId>,
}

impl Graph {
    fn add(&mut self, kind: NodeKind) -> NodeId {
        self.nodes.push(Node { kind, previous: Vec::new() });
        self.nodes.len() - 1
    }

    fn link(&mut self, from: NodeId, to: NodeId) {
        self.nodes[to].previous.push(from);
    }

    fn instruction(&self, n: NodeId) -> Option<InstrRef> {
        match self.nodes[n].kind {
            NodeKind::Instruction(r) => Some(r),
            _ => None,
        }
    }
}

struct Analysis {
    graph: Rc<Graph>,
    required: Vec<Vec<NodeId>>,
    set_nodes: Vec<HashSet<NodeId>>,
    corrupted_nodes: Vec<HashSet<NodeId>>,
    returns: Vec<NodeId>,
    static_label_args: HashMap<InstrRef, IndexMap<usize, SymbolId>>,
}

struct PointerChecker<'a> {
    c: &'a ServerScriptCompiler,
    scripts: &'a [RuneScript],
    d: &'a mut Diagnostics,
    features: StrictFeatureLevel,
    overlay_interfaces: HashSet<String>,
    scripts_by_symbol: HashMap<SymbolId, usize>,
    graphs: HashMap<SymbolId, Rc<Graph>>,
    pointers: HashMap<SymbolId, Rc<PointerHolder>>,
    analyses: HashMap<SymbolId, Rc<Analysis>>,
    jump_param_nodes: HashMap<SymbolId, Rc<IndexMap<usize, Vec<NodeId>>>>,
    pending_analyses: HashSet<SymbolId>,
    pending_scripts: HashSet<SymbolId>,
}

impl PointerChecker<'_> {
    fn types(&self) -> &TypeManager {
        &self.c.types
    }

    fn script_name(&self, s: SymbolId) -> &str {
        self.c.tables[s].name()
    }

    fn holder(&self, command: SymbolId) -> Option<&PointerHolder> {
        self.c.command_pointers.get(self.script_name(command))
    }

    // ------------------------------------------------------------ graph generation

    fn graph(&mut self, script: usize) -> R<Rc<Graph>> {
        let symbol = self.scripts[script].symbol;
        if let Some(g) = self.graphs.get(&symbol) {
            return Ok(g.clone());
        }
        let g = Rc::new(self.generate(&self.scripts[script])?);
        self.graphs.insert(symbol, g.clone());
        Ok(g)
    }

    fn is_conditional_setter(&self, script: &RuneScript, r: InstrRef) -> bool {
        let inst = &script.blocks[r.0].instructions[r.1];
        match (inst.opcode, &inst.operand) {
            (Opcode::Command, Operand::Symbol(s)) => self.holder(*s).is_some_and(|h| h.conditional_set),
            _ => false,
        }
    }

    /// A conditional setter followed by `push value` (1, or 0 when inverted).
    fn check_conditional(&self, script: &RuneScript, block: usize, idx: usize, value: i32) -> bool {
        if idx < 2 || !self.is_conditional_setter(script, (block, idx - 2)) {
            return false;
        }
        let prev = &script.blocks[block].instructions[idx - 1];
        prev.opcode == Opcode::PushConstantInt && prev.operand == Operand::Int(value)
    }

    fn generate(&self, script: &RuneScript) -> R<Graph> {
        let blocks = &script.blocks;
        let label_to_block: HashMap<&str, usize> = blocks.iter().enumerate().map(|(i, b)| (b.label.name.as_str(), i)).collect();
        let mut g = Graph::default();
        let mut cache: HashMap<InstrRef, NodeId> = HashMap::new();
        let get_or_create = |g: &mut Graph, cache: &mut HashMap<InstrRef, NodeId>, r: InstrRef| -> NodeId {
            *cache.entry(r).or_insert_with(|| g.add(NodeKind::Instruction(r)))
        };
        // The first instruction of `block`, or of the next non-empty block after it.
        let first_from = |block: usize| (block..blocks.len()).find(|&b| !blocks[b].instructions.is_empty()).map(|b| (b, 0));
        let first_instruction = |g: &mut Graph, cache: &mut HashMap<InstrRef, NodeId>, block: usize| -> R<NodeId> {
            match first_from(block) {
                Some(r) => Ok(get_or_create(g, cache, r)),
                None => fatal("No instructions remaining."),
            }
        };
        // Fall-through successor: the next instruction, else the next non-empty
        // block's first (`None` when only empty blocks follow).
        let fall_through = |block: usize, idx: usize| -> R<Option<InstrRef>> {
            if idx + 1 < blocks[block].instructions.len() {
                Ok(Some((block, idx + 1)))
            } else if block + 1 < blocks.len() {
                Ok(first_from(block + 1))
            } else {
                fatal("No next instruction")
            }
        };
        let label_block = |l: &str| -> R<usize> { label_to_block.get(l).copied().map_or_else(|| fatal("Unable to find block."), Ok) };
        let setter_set = |r: InstrRef| -> IndexSet<PointerType> {
            let Operand::Symbol(s) = &blocks[r.0].instructions[r.1].operand else { unreachable!("command") };
            self.holder(*s).expect("conditional setter has a holder").set.clone()
        };
        let terminal = |op: Opcode| matches!(op, Opcode::Branch | Opcode::Jump | Opcode::Return);

        let start = g.add(NodeKind::Start);
        let first = first_instruction(&mut g, &mut cache, 0)?;
        g.link(start, first);
        g.list.push(start);
        let mut potential = false;
        for (bi, block) in blocks.iter().enumerate() {
            for (ii, inst) in block.instructions.iter().enumerate() {
                let node = get_or_create(&mut g, &mut cache, (bi, ii));
                g.list.push(node);
                if potential && inst.opcode == Opcode::BranchEquals && self.check_conditional(script, bi, ii, 0) {
                    if self.features.pointer_inversion {
                        if ii + 1 >= block.instructions.len() {
                            return fatal("Invalid inverted conditional layout");
                        }
                        let next = get_or_create(&mut g, &mut cache, (bi, ii + 1));
                        if block.instructions[ii + 1].opcode != Opcode::Branch {
                            return fatal("Expected Branch opcode");
                        }
                        if block.instructions[ii - 2].opcode != Opcode::Command {
                            return fatal("Expected command before conditional");
                        }
                        let setter = g.add(NodeKind::Pointer(setter_set((bi, ii - 2))));
                        g.list.push(setter);
                        g.link(node, setter);
                        g.link(setter, next);
                    } else if !terminal(inst.opcode)
                        && let Some(next) = fall_through(bi, ii)?
                    {
                        let next = get_or_create(&mut g, &mut cache, next);
                        g.link(node, next);
                    }
                    potential = false;
                } else if !terminal(inst.opcode)
                    && let Some(next) = fall_through(bi, ii)?
                {
                    let next = get_or_create(&mut g, &mut cache, next);
                    g.link(node, next);
                }

                if potential && inst.opcode == Opcode::BranchEquals && self.check_conditional(script, bi, ii, 1) {
                    let Operand::Label(l) = &inst.operand else { unreachable!("branch label") };
                    let jump = label_block(&l.name)?;
                    if block.instructions[ii - 2].opcode != Opcode::Command {
                        return fatal("Expected command before conditional.");
                    }
                    let setter = g.add(NodeKind::Pointer(setter_set((bi, ii - 2))));
                    g.list.push(setter);
                    g.link(node, setter);
                    let target = first_instruction(&mut g, &mut cache, jump)?;
                    g.link(setter, target);
                    potential = false;
                } else if is_branch(inst.opcode) {
                    let Operand::Label(l) = &inst.operand else { unreachable!("branch label") };
                    let jump = label_block(&l.name)?;
                    let target = first_instruction(&mut g, &mut cache, jump)?;
                    g.link(node, target);
                } else if inst.opcode == Opcode::Switch {
                    let Operand::Switch(id) = inst.operand else { unreachable!("switch table") };
                    for case in &script.switch_tables[id].cases {
                        if case.keys.is_empty() {
                            continue;
                        }
                        let jump = label_block(&case.label.name)?;
                        let target = first_instruction(&mut g, &mut cache, jump)?;
                        g.link(node, target);
                    }
                }
                if self.is_conditional_setter(script, (bi, ii)) {
                    potential = true;
                }
            }
        }
        Ok(g)
    }

    // ------------------------------------------------------------ analysis

    fn script_of(&self, symbol: SymbolId) -> R<usize> {
        self.scripts_by_symbol.get(&symbol).copied().map_or_else(|| fatal("Unable to find script from symbol."), Ok)
    }

    /// Caches whatever the calculation returns, including the
    /// empty holder handed out for a script still being calculated.
    fn get_pointers(&mut self, symbol: SymbolId) -> R<Rc<PointerHolder>> {
        if let Some(p) = self.pointers.get(&symbol) {
            return Ok(p.clone());
        }
        let p = Rc::new(self.calculate_pointers(symbol)?);
        self.pointers.insert(symbol, p.clone());
        Ok(p)
    }

    fn calculate_pointers(&mut self, symbol: SymbolId) -> R<PointerHolder> {
        if self.pending_scripts.contains(&symbol) {
            return Ok(PointerHolder::default());
        }
        let script = self.script_of(symbol)?;
        let mut holder = PointerHolder::default();
        self.pending_scripts.insert(symbol);
        for &p in PointerType::ALL {
            if self.requires_path(script, p)?.is_some() {
                holder.required.insert(p);
            }
            let a = self.analysis(script)?;
            let i = p.index();
            let start = a.graph.list[0];
            if self.find_edge_path(&a.graph, &a.returns, |n| n == start || a.corrupted_nodes[i].contains(&n), &a.set_nodes[i]).is_none() {
                holder.set.insert(p);
            }
            let a = self.analysis(script)?;
            if self.find_edge_path(&a.graph, &a.returns, |n| a.corrupted_nodes[i].contains(&n), &a.set_nodes[i]).is_some() {
                holder.corrupted.insert(p);
            }
        }
        self.pending_scripts.remove(&symbol);
        Ok(holder)
    }

    fn requires_path(&mut self, script: usize, p: PointerType) -> R<Option<Vec<NodeId>>> {
        let a = self.analysis(script)?;
        let start = a.graph.list[0];
        Ok(self.find_edge_path(&a.graph, &a.required[p.index()], |n| n == start, &a.set_nodes[p.index()]))
    }

    fn analysis(&mut self, script: usize) -> R<Rc<Analysis>> {
        let symbol = self.scripts[script].symbol;
        if let Some(a) = self.analyses.get(&symbol) {
            return Ok(a.clone());
        }
        let graph = self.graph(script)?;
        if self.pending_analyses.contains(&symbol) {
            return Ok(Rc::new(empty_analysis(graph, &self.scripts[script])));
        }
        self.pending_analyses.insert(symbol);
        let result = self.build_analysis(script, graph);
        self.pending_analyses.remove(&symbol);
        let a = Rc::new(result?);
        self.analyses.insert(symbol, a.clone());
        Ok(a)
    }

    fn build_analysis(&mut self, script: usize, graph: Rc<Graph>) -> R<Analysis> {
        let s = &self.scripts[script];
        let static_label_args = self.static_label_args(s);
        self.jump_params(script)?;
        let mut required = vec![Vec::new(); POINTERS];
        let mut set = vec![Vec::new(); POINTERS];
        let mut corrupted = vec![Vec::new(); POINTERS];
        let mut returns = Vec::new();
        let add_all = |target: &mut Vec<Vec<NodeId>>, pointers: &IndexSet<PointerType>, n: NodeId| {
            for p in pointers {
                target[p.index()].push(n);
            }
        };
        for &n in &graph.list {
            if let NodeKind::Pointer(ps) = &graph.nodes[n].kind {
                add_all(&mut set, ps, n);
                continue;
            }
            let Some(r) = graph.instruction(n) else { continue };
            let inst = &s.blocks[r.0].instructions[r.1];
            if inst.opcode == Opcode::Return {
                returns.push(n);
            }
            match (inst.opcode, &inst.operand) {
                (Opcode::Command, Operand::Symbol(sym)) => {
                    if let Some(h) = self.holder(*sym) {
                        add_all(&mut required, &h.required, n);
                        add_all(&mut corrupted, &h.corrupted, n);
                        if !h.conditional_set {
                            add_all(&mut set, &h.set, n);
                        }
                    }
                }
                (Opcode::Gosub | Opcode::Jump, Operand::Symbol(sym)) => {
                    let h = self.get_pointers(*sym)?;
                    add_all(&mut required, &h.required, n);
                    add_all(&mut set, &h.set, n);
                    add_all(&mut corrupted, &h.corrupted, n);
                    if let Some(args) = static_label_args.get(&r) {
                        self.add_static_label_requirements(&mut required, n, *sym, args)?;
                    }
                }
                (op @ (Opcode::PushVar | Opcode::PopVar | Opcode::PushVar2 | Opcode::PopVar2), Operand::Symbol(sym)) => {
                    let Symbol::Basic { ty, is_protected, .. } = self.c.tables[*sym] else { continue };
                    let kind = &self.types()[ty].kind;
                    let player = matches!(kind, TypeKind::VarPlayer { .. } | TypeKind::VarBit { .. });
                    let npc = matches!(kind, TypeKind::VarNpc { .. });
                    let obj = matches!(kind, TypeKind::VarObj { .. });
                    let p = match (op, player, npc) {
                        (Opcode::PushVar, true, _) => Some(PointerType::ActivePlayer),
                        (Opcode::PopVar, true, _) => Some(if is_protected { PointerType::PActivePlayer } else { PointerType::ActivePlayer }),
                        (Opcode::PushVar2, true, _) => Some(PointerType::ActivePlayer2),
                        (Opcode::PopVar2, true, _) => Some(if is_protected { PointerType::PActivePlayer2 } else { PointerType::ActivePlayer2 }),
                        (Opcode::PushVar | Opcode::PopVar, _, true) => Some(PointerType::ActiveNpc),
                        (_, _, true) => Some(PointerType::ActiveNpc2),
                        (Opcode::PushVar | Opcode::PopVar, _, _) if obj => Some(PointerType::ActiveInvObj),
                        (_, _, _) if obj => Some(PointerType::ActiveInvObj2),
                        _ => None,
                    };
                    if let Some(p) = p {
                        required[p.index()].push(n);
                    }
                }
                _ => {}
            }
        }
        Ok(Analysis {
            graph,
            set_nodes: set.iter().map(|v| v.iter().copied().collect()).collect(),
            corrupted_nodes: corrupted.iter().map(|v| v.iter().copied().collect()).collect(),
            required,
            returns,
            static_label_args,
        })
    }

    fn is_label_type(&self, t: TypeId) -> bool {
        matches!(self.types()[t].kind, TypeKind::Script { trigger: triggers::LABEL, .. })
    }

    /// Per gosub/jump, label-typed parameters whose
    /// argument is a pushed label symbol.
    fn static_label_args(&self, s: &RuneScript) -> HashMap<InstrRef, IndexMap<usize, SymbolId>> {
        const ARG_PUSH: [Opcode; 7] = [
            Opcode::PushConstantInt,
            Opcode::PushConstantString,
            Opcode::PushConstantLong,
            Opcode::PushConstantSymbol,
            Opcode::PushLocalVar,
            Opcode::PushVar,
            Opcode::PushVar2,
        ];
        let mut result = HashMap::new();
        for (bi, block) in s.blocks.iter().enumerate() {
            for (ii, inst) in block.instructions.iter().enumerate() {
                let (Opcode::Gosub | Opcode::Jump, Operand::Symbol(sym)) = (inst.opcode, &inst.operand) else { continue };
                let Symbol::Script { parameters, .. } = self.c.tables[*sym] else { continue };
                let params = self.types().to_list(Some(parameters));
                if params.is_empty() {
                    continue;
                }
                // The argument pushes right before the call.
                let pushes: Vec<_> = block.instructions[..ii].iter().rev().take(params.len()).collect();
                if pushes.len() != params.len() || pushes.iter().any(|i| !ARG_PUSH.contains(&i.opcode)) {
                    continue;
                }
                let mut args = IndexMap::new();
                for (pi, (&t, arg)) in params.iter().zip(pushes.iter().rev()).enumerate() {
                    if !self.is_label_type(t) || arg.opcode != Opcode::PushConstantSymbol {
                        continue;
                    }
                    if let Operand::Symbol(a) = arg.operand
                        && matches!(self.c.tables[a], Symbol::Script { trigger: triggers::LABEL, .. })
                    {
                        args.insert(pi, a);
                    }
                }
                if !args.is_empty() {
                    result.insert((bi, ii), args);
                }
            }
        }
        result
    }

    /// `jump`/`.jump` commands whose argument is a
    /// label-typed parameter, by parameter index.
    fn jump_params(&mut self, script: usize) -> R<Rc<IndexMap<usize, Vec<NodeId>>>> {
        let s = &self.scripts[script];
        if let Some(c) = self.jump_param_nodes.get(&s.symbol) {
            return Ok(c.clone());
        }
        let graph = self.graph(script)?;
        let node_of: HashMap<InstrRef, NodeId> = graph.list.iter().filter_map(|&n| graph.instruction(n).map(|r| (r, n))).collect();
        let mut out: IndexMap<usize, Vec<NodeId>> = IndexMap::new();
        for (bi, block) in s.blocks.iter().enumerate() {
            for (ii, inst) in block.instructions.iter().enumerate() {
                let (Opcode::Command, Operand::Symbol(cmd)) = (inst.opcode, &inst.operand) else { continue };
                if !matches!(self.script_name(*cmd), "jump" | ".jump") || ii == 0 {
                    continue;
                }
                let prev = &block.instructions[ii - 1];
                let (Opcode::PushLocalVar, Operand::Symbol(local)) = (prev.opcode, &prev.operand) else { continue };
                let Some(pi) = s.parameters.iter().position(|p| p == local) else { continue };
                let Symbol::LocalVariable { ty, .. } = self.c.tables[s.parameters[pi]] else { continue };
                if !self.is_label_type(ty) {
                    continue;
                }
                if let Some(&n) = node_of.get(&(bi, ii)) {
                    out.entry(pi).or_default().push(n);
                }
            }
        }
        let out = Rc::new(out);
        self.jump_param_nodes.insert(s.symbol, out.clone());
        Ok(out)
    }

    fn requires_at_nodes(&mut self, script: usize, p: PointerType, nodes: &[NodeId]) -> R<bool> {
        if nodes.is_empty() {
            return Ok(false);
        }
        let a = self.analysis(script)?;
        let start = a.graph.list[0];
        Ok(self.find_edge_path(&a.graph, nodes, |n| n == start, &a.set_nodes[p.index()]).is_some())
    }

    fn add_static_label_requirements(&mut self, required: &mut [Vec<NodeId>], caller: NodeId, called: SymbolId, args: &IndexMap<usize, SymbolId>) -> R {
        let Some(&called_script) = self.scripts_by_symbol.get(&called) else { return Ok(()) };
        let params = self.jump_params(called_script)?;
        if params.is_empty() {
            return Ok(());
        }
        for (pi, &label) in args {
            let Some(nodes) = params.get(pi).filter(|n| !n.is_empty()) else { continue };
            let label_pointers = self.get_pointers(label)?;
            for &p in &label_pointers.required {
                if self.requires_at_nodes(called_script, p, nodes)? {
                    required[p.index()].push(caller);
                }
            }
        }
        Ok(())
    }

    /// BFS backwards (over `previous`) from the predecessors of
    /// `starts`; returns `[start, …, end]`.
    fn find_edge_path(&self, g: &Graph, starts: &[NodeId], end: impl Fn(NodeId) -> bool, blocked: &HashSet<NodeId>) -> Option<Vec<NodeId>> {
        let mut sources: HashMap<NodeId, Option<NodeId>> = HashMap::new();
        let mut start_source: HashMap<NodeId, NodeId> = HashMap::new();
        let mut queue = Vec::new();
        for &start in starts {
            for &n in &g.nodes[start].previous {
                if blocked.contains(&n) || sources.contains_key(&n) {
                    continue;
                }
                start_source.insert(n, start);
                sources.insert(n, None);
                queue.push(n);
            }
        }
        let mut i = 0;
        while i < queue.len() {
            let current = queue[i];
            i += 1;
            if end(current) {
                let mut path = vec![current];
                let mut node = sources[&current];
                while let Some(n) = node {
                    path.push(n);
                    node = sources[&n];
                }
                path.reverse();
                path.insert(0, start_source[&path[0]]);
                return Some(path);
            }
            for &n in &g.nodes[current].previous {
                if blocked.contains(&n) || sources.contains_key(&n) {
                    continue;
                }
                sources.insert(n, Some(current));
                queue.push(n);
            }
        }
        None
    }

    // ------------------------------------------------------------ validation

    /// `ServerPointerChecker.setsPointerTrigger`.
    fn trigger_sets(&self, script: &RuneScript, p: PointerType) -> bool {
        let trigger = script.trigger.get();
        let button_ids = [triggers::INV_BUTTON1, triggers::INV_BUTTON2, triggers::INV_BUTTON3, triggers::INV_BUTTON4, triggers::INV_BUTTON5, triggers::INV_BUTTOND]
            .map(|t| t.get().id);
        // `IF_BUTTON` by identity; the inv buttons by numeric id (`if_buttonX` aliases).
        if p == PointerType::PActivePlayer && (script.trigger == triggers::IF_BUTTON || button_ids.contains(&trigger.id)) {
            let Some(subject) = script.subject_reference else { return false };
            let interface = self.c.tables[subject].name().split(':').next().unwrap_or("");
            if interface.is_empty() {
                return false;
            }
            return !self.overlay_interfaces.contains(&normalize_basic(interface));
        }
        trigger.pointers.is_some_and(|ps| ps.contains(&p))
    }

    fn source(&self, script: usize, g: &Graph, n: NodeId, message: &str) -> R<Loc> {
        g.instruction(n)
            .and_then(|r| self.scripts[script].blocks[r.0].instructions[r.1].source.clone())
            .map_or_else(|| fatal(message), Ok)
    }

    /// A `note:` on the pointer error just reported.
    fn note(&mut self, loc: &Loc, message: String) {
        if let Some(d) = self.d.list.last_mut() {
            d.note(Some(loc), message);
        }
    }

    fn validate_pointer(&mut self, script: usize, p: PointerType) -> R {
        let a = self.analysis(script)?;
        let i = p.index();
        let graph = &a.graph;
        let start = graph.list[0];
        let mut corrupted_set = a.corrupted_nodes[i].clone();
        if !self.trigger_sets(&self.scripts[script], p) && !graph.list.is_empty() {
            corrupted_set.insert(start);
        }
        let Some(path) = self.find_edge_path(graph, &a.required[i], |n| corrupted_set.contains(&n), &a.set_nodes[i]) else { return Ok(()) };
        let error_node = path[0];
        let error_location = self.source(script, graph, error_node, "Unknown instruction source.")?;
        let corrupted_node = *path.last().unwrap();
        let is_corrupted = corrupted_node != start && corrupted_node != error_node;
        let message = if is_corrupted { msg::POINTER_CORRUPTED } else { msg::POINTER_UNINITIALIZED };
        let rep = p.representation();
        self.d.report(DiagnosticType::Error, &error_location, message, vec![rep.to_owned()]);
        if is_corrupted {
            let loc = self.source(script, graph, corrupted_node, "Unknown instruction source.")?;
            self.note(&loc, msg::POINTER_CORRUPTED_LOC.replace("%s", rep));
        }
        self.log_proc_requirement((script, &a), script, error_node, p, 0)
    }

    /// Notes down the chain of calls that require `p`. Label arguments are
    /// always looked up in the *validated* script's static label arguments
    /// (keyed by instruction), so the lookup only succeeds while `script` is
    /// that script. A cyclic chain would recurse forever; `depth` turns that
    /// into a fatal error.
    fn log_proc_requirement(&mut self, top: (usize, &Analysis), script: usize, node: NodeId, p: PointerType, depth: usize) -> R {
        if depth > 10_000 {
            return fatal("Pointer requirement chain is too deep.");
        }
        let g = self.graph(script)?;
        let Some(r) = g.instruction(node) else { return Ok(()) };
        let inst = &self.scripts[script].blocks[r.0].instructions[r.1];
        let (Opcode::Gosub | Opcode::Jump, Operand::Symbol(called)) = (inst.opcode, &inst.operand) else { return Ok(()) };
        let called_script = self.script_of(*called).or_else(|_| fatal("Unable to find script."))?;
        let rep = msg::POINTER_REQUIRED_LOC.replace("%s", p.representation());
        match self.requires_path(called_script, p)? {
            None => {
                let args = if top.0 == script { top.1.static_label_args.get(&r) } else { None };
                let Some(args) = args else { return Ok(()) };
                let params = self.jump_params(called_script)?;
                for (pi, &label) in args {
                    if !self.get_pointers(label)?.required.contains(&p) {
                        continue;
                    }
                    let Some(nodes) = params.get(pi).filter(|n| !n.is_empty()) else { continue };
                    if !self.requires_at_nodes(called_script, p, nodes)? {
                        continue;
                    }
                    let g = self.graph(called_script)?;
                    let loc = self.source(called_script, &g, nodes[0], "Invalid instruction/source.")?;
                    self.note(&loc, rep.clone());
                }
                Ok(())
            }
            Some(path) => {
                let g = self.graph(called_script)?;
                let loc = self.source(called_script, &g, path[0], "Invalid instruction/source.")?;
                self.note(&loc, rep);
                self.log_proc_requirement(top, called_script, path[0], p, depth + 1)
            }
        }
    }
}

/// No pointer data, only the returns.
fn empty_analysis(graph: Rc<Graph>, script: &RuneScript) -> Analysis {
    let returns = graph
        .list
        .iter()
        .copied()
        .filter(|&n| graph.instruction(n).is_some_and(|r| script.blocks[r.0].instructions[r.1].opcode == Opcode::Return))
        .collect();
    Analysis {
        graph,
        required: vec![Vec::new(); POINTERS],
        set_nodes: vec![HashSet::new(); POINTERS],
        corrupted_nodes: vec![HashSet::new(); POINTERS],
        returns,
        static_label_args: HashMap::new(),
    }
}

fn is_branch(op: Opcode) -> bool {
    use Opcode::*;
    matches!(
        op,
        Branch
            | BranchNot
            | BranchEquals
            | BranchLessThan
            | BranchGreaterThan
            | BranchLessThanOrEquals
            | BranchGreaterThanOrEquals
            | LongBranchNot
            | LongBranchEquals
            | LongBranchLessThan
            | LongBranchGreaterThan
            | LongBranchLessThanOrEquals
            | LongBranchGreaterThanOrEquals
            | ObjBranchNot
            | ObjBranchEquals
    )
}
