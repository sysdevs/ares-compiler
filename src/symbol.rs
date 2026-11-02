//! Symbols, symbol types and scoped symbol tables.
//!
//! Symbols are compared by identity (the id mapper and AST references key on
//! them), so they live in an arena and are referred to by [`SymbolId`].
//! A `SymbolType` is a value keyed by [`TriggerId`] / [`TypeId`], which are
//! identities already.

use std::collections::HashMap;
use std::ops::Index;

use indexmap::IndexMap;

use crate::ast::Loc;
use crate::trigger::TriggerId;
use crate::types::TypeId;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SymbolId(u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ScopeId(u32);

#[derive(Clone, PartialEq, Debug)]
pub enum Symbol {
    LocalVariable { name: String, ty: TypeId },
    Basic { name: String, ty: TypeId, is_protected: bool },
    Constant { name: String, value: String },
    Script { trigger: TriggerId, name: String, parameters: TypeId, returns: TypeId },
}

impl Symbol {
    pub fn name(&self) -> &str {
        match self {
            Symbol::LocalVariable { name, .. }
            | Symbol::Basic { name, .. }
            | Symbol::Constant { name, .. }
            | Symbol::Script { name, .. } => name,
        }
    }
}

/// The bucket a symbol is inserted under.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SymbolType {
    ServerScript(TriggerId),
    LocalVariable,
    Basic(TypeId),
    Constant,
}

#[derive(Default, Debug)]
struct Scope {
    parent: Option<ScopeId>,
    /// Insertion order of the outer map is the symbol type order.
    tables: IndexMap<SymbolType, IndexMap<String, SymbolId>>,
    /// Per name, `(type order, symbol)` sorted by order.
    basic_by_name: HashMap<String, Vec<(usize, SymbolId)>>,
    exact_by_name: HashMap<String, Vec<(usize, SymbolId)>>,
}

/// All symbols plus the symbol table tree (one [`Scope`] per table).
#[derive(Debug)]
pub struct SymbolTables {
    symbols: Vec<Symbol>,
    scopes: Vec<Scope>,
    /// Where scripts and locals were declared, for `defined here` notes.
    decls: HashMap<SymbolId, Loc>,
}

impl Default for SymbolTables {
    fn default() -> Self {
        Self { symbols: Vec::new(), scopes: vec![Scope::default()], decls: HashMap::new() }
    }
}

impl Index<SymbolId> for SymbolTables {
    type Output = Symbol;
    fn index(&self, id: SymbolId) -> &Symbol {
        &self.symbols[id.0 as usize]
    }
}

/// A basic symbol's lookup key: lowercased, each whitespace run → `_`.
pub(crate) fn normalize_basic(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut in_space = false;
    for c in lower.chars() {
        if c.is_whitespace() {
            if !in_space {
                out.push('_');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

impl SymbolTables {
    /// The root table.
    pub const ROOT: ScopeId = ScopeId(0);

    /// Allocates a symbol; it is not in any table yet.
    pub fn add(&mut self, symbol: Symbol) -> SymbolId {
        self.symbols.push(symbol);
        SymbolId(self.symbols.len() as u32 - 1)
    }

    pub fn declared_at(&mut self, symbol: SymbolId, source: &Loc) {
        self.decls.insert(symbol, source.clone());
    }

    pub fn decl(&self, symbol: SymbolId) -> Option<&Loc> {
        self.decls.get(&symbol)
    }

    /// A new child table of `parent`.
    pub fn sub_table(&mut self, parent: ScopeId) -> ScopeId {
        self.scopes.push(Scope { parent: Some(parent), ..Scope::default() });
        ScopeId(self.scopes.len() as u32 - 1)
    }

    fn key(ty: SymbolType, name: &str) -> String {
        match ty {
            SymbolType::Basic(_) => normalize_basic(name),
            _ => name.to_owned(),
        }
    }

    fn parents(&self, scope: ScopeId) -> impl Iterator<Item = &Scope> {
        std::iter::successors(Some(&self.scopes[scope.0 as usize]), |s| s.parent.map(|p| &self.scopes[p.0 as usize]))
    }

    /// False if `scope` or any parent already has the name under `ty`.
    pub fn insert(&mut self, scope: ScopeId, ty: SymbolType, symbol: SymbolId) -> bool {
        let key = Self::key(ty, self[symbol].name());
        if self.parents(scope).any(|s| s.tables.get(&ty).is_some_and(|t| t.contains_key(&key))) {
            return false;
        }
        let s = &mut self.scopes[scope.0 as usize];
        let entry = s.tables.entry(ty);
        let order = entry.index();
        entry.or_default().insert(key.clone(), symbol);
        let index = if let SymbolType::Basic(_) = ty { &mut s.basic_by_name } else { &mut s.exact_by_name };
        let matches = index.entry(key).or_default();
        let at = matches.iter().position(|&(o, _)| o > order).unwrap_or(matches.len());
        matches.insert(at, (order, symbol));
        true
    }

    /// Nearest scope first.
    pub fn find(&self, scope: ScopeId, ty: SymbolType, name: &str) -> Option<SymbolId> {
        let key = Self::key(ty, name);
        self.parents(scope).find_map(|s| s.tables.get(&ty).and_then(|t| t.get(&key)).copied())
    }

    /// Every key under `ty` visible from `scope` (basic names normalised), nearest scope first.
    pub fn names(&self, scope: ScopeId, ty: SymbolType) -> impl Iterator<Item = &str> {
        self.parents(scope).flat_map(move |s| s.tables.get(&ty).into_iter().flat_map(|t| t.keys().map(String::as_str)))
    }

    /// `scope`'s own buckets in `SymbolType` order, each with its (normalised
    /// key, symbol) entries in insertion order.
    pub fn buckets(&self, scope: ScopeId) -> impl Iterator<Item = (SymbolType, impl Iterator<Item = (&str, SymbolId)>)> {
        self.scopes[scope.0 as usize].tables.iter().map(|(ty, t)| (*ty, t.iter().map(|(k, v)| (k.as_str(), *v))))
    }

    /// Per scope (nearest first), basic and exact matches
    /// merged by the order their `SymbolType` was first inserted into that scope.
    pub fn find_all(&self, scope: ScopeId, name: &str) -> Vec<SymbolId> {
        let mut out = Vec::new();
        let basic_key = normalize_basic(name);
        for s in self.parents(scope) {
            let basic = s.basic_by_name.get(&basic_key).map_or(&[][..], Vec::as_slice);
            let exact = s.exact_by_name.get(name).map_or(&[][..], Vec::as_slice);
            let (mut b, mut e) = (0, 0);
            while b < basic.len() || e < exact.len() {
                if e >= exact.len() || (b < basic.len() && basic[b].0 < exact[e].0) {
                    out.push(basic[b].1);
                    b += 1;
                } else {
                    out.push(exact[e].1);
                    e += 1;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runescript::triggers;
    use crate::types::{INT, STRING};

    #[test]
    fn normalizes_basic_names_only() {
        assert_eq!(normalize_basic("Foo  Bar\u{85}Baz"), "foo_bar_baz");
        assert_eq!(normalize_basic("a\u{FEFF}b"), "a\u{FEFF}b", "a BOM is not whitespace");
        let mut t = SymbolTables::default();
        let b = t.add(Symbol::Basic { name: "Cook Fish".into(), ty: INT, is_protected: false });
        assert!(t.insert(SymbolTables::ROOT, SymbolType::Basic(INT), b));
        assert_eq!(t.find(SymbolTables::ROOT, SymbolType::Basic(INT), "cook_fish"), Some(b));
        let c = t.add(Symbol::Constant { name: "Cook".into(), value: "1".into() });
        assert!(t.insert(SymbolTables::ROOT, SymbolType::Constant, c));
        assert_eq!(t.find(SymbolTables::ROOT, SymbolType::Constant, "cook"), None);
    }

    #[test]
    fn duplicates_rejected_per_type_and_up_the_chain() {
        let mut t = SymbolTables::default();
        let a = t.add(Symbol::Basic { name: "x".into(), ty: INT, is_protected: false });
        let a2 = t.add(Symbol::Basic { name: "X".into(), ty: INT, is_protected: false });
        let s = t.add(Symbol::Basic { name: "x".into(), ty: STRING, is_protected: false });
        assert!(t.insert(SymbolTables::ROOT, SymbolType::Basic(INT), a));
        assert!(!t.insert(SymbolTables::ROOT, SymbolType::Basic(INT), a2));
        assert!(t.insert(SymbolTables::ROOT, SymbolType::Basic(STRING), s), "other bucket");
        let child = t.sub_table(SymbolTables::ROOT);
        assert!(!t.insert(child, SymbolType::Basic(INT), a2), "parent has it");
        let l = t.add(Symbol::LocalVariable { name: "x".into(), ty: INT });
        assert!(t.insert(child, SymbolType::LocalVariable, l));
        assert_eq!(t.find(child, SymbolType::Basic(STRING), "x"), Some(s));
        assert_eq!(t.find(SymbolTables::ROOT, SymbolType::LocalVariable, "x"), None);
    }

    #[test]
    fn find_all_merges_by_first_insert_order_then_parents() {
        let mut t = SymbolTables::default();
        let root = SymbolTables::ROOT;
        // Type order in root: Basic(STRING)=0, ServerScript(PROC)=1, Basic(INT)=2.
        let s = t.add(Symbol::Basic { name: "zzz".into(), ty: STRING, is_protected: false });
        t.insert(root, SymbolType::Basic(STRING), s);
        let p = t.add(Symbol::Script { trigger: triggers::PROC, name: "x".into(), parameters: INT, returns: INT });
        t.insert(root, SymbolType::ServerScript(triggers::PROC), p);
        let i = t.add(Symbol::Basic { name: "X".into(), ty: INT, is_protected: false });
        t.insert(root, SymbolType::Basic(INT), i);
        let s2 = t.add(Symbol::Basic { name: "x".into(), ty: STRING, is_protected: false });
        t.insert(root, SymbolType::Basic(STRING), s2);
        let child = t.sub_table(root);
        let l = t.add(Symbol::LocalVariable { name: "x".into(), ty: INT });
        t.insert(child, SymbolType::LocalVariable, l);
        assert_eq!(t.find_all(child, "x"), [l, s2, p, i]);
        // Exact lookup is case-sensitive, basic is not.
        assert_eq!(t.find_all(root, "X"), [s2, i]);
    }
}
