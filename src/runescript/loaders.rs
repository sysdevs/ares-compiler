//! Fill the root table from `symbols.json`: constants in file order, every
//! other table in id order ([`CompilerTypeInfo::ids`]).

use crate::runescript::symbol_mapper::SymbolMapper;
use crate::runescript::type_info::CompilerTypeInfo;
use crate::symbol::{Symbol, SymbolTables, SymbolType};
use crate::runescript::types as sv;
use crate::types::{TypeId, TypeManager, CHAR, INT, UNIT};
use crate::FatalError;

/// How a loaded symbol's type is built from its vartype.
#[derive(Clone, Copy, Debug)]
pub enum TypeSupplier {
    /// Always this type; the vartype is ignored.
    Fixed(TypeId),
    Param,
    VarPlayer,
    VarNpc,
    VarObj,
    VarShared,
    VarClient,
    VarBit,
    DbColumn,
}

impl TypeSupplier {
    fn supply(self, types: &mut TypeManager, sub: TypeId) -> TypeId {
        match self {
            TypeSupplier::Fixed(t) => t,
            TypeSupplier::Param => types.param(sub),
            TypeSupplier::VarPlayer => types.var_player(sub),
            TypeSupplier::VarNpc => types.var_npc(sub),
            TypeSupplier::VarObj => types.var_obj(sub),
            TypeSupplier::VarShared => types.var_shared(sub),
            TypeSupplier::VarClient => types.var_client(sub),
            TypeSupplier::VarBit => types.var_bit(sub),
            TypeSupplier::DbColumn => types.db_column(sub),
        }
    }
}

#[derive(Clone, Debug)]
pub enum SymbolLoader {
    /// `map` is name → value.
    Constant(CompilerTypeInfo),
    /// `map` is id → name; `protected` loaders also read `protect`.
    Basic { info: CompilerTypeInfo, supplier: TypeSupplier, protected: bool },
    /// The client dialect's `event_*` values (see [`EVENTS`]).
    Event,
}

/// `event_*` names a client hook argument can take: `(name, magic offset, type)`.
/// Each is a constant `i32::MIN + offset` the client replaces with the event's
/// value; `event_opbase` is the string `"event_opbase"`.
pub const EVENTS: &[(&str, i32, TypeId)] = &[
    ("event_mousex", 1, INT),
    ("event_mousey", 2, INT),
    ("event_com", 3, sv::COMPONENT),
    ("event_op", 4, INT),
    ("event_comsubid", 5, INT),
    ("event_com2", 6, sv::COMPONENT),
    ("event_comsubid2", 7, INT),
    ("event_keycode", 8, INT),
    ("event_keychar", 9, CHAR),
];

impl SymbolLoader {
    pub fn load(&self, tables: &mut SymbolTables, types: &mut TypeManager, mapper: &mut SymbolMapper) -> Result<(), FatalError> {
        match self {
            SymbolLoader::Constant(info) => {
                for (name, value) in &info.map {
                    let symbol = tables.add(Symbol::Constant { name: name.to_owned(), value: value.clone() });
                    if !tables.insert(SymbolTables::ROOT, SymbolType::Constant, symbol) {
                        return Err(FatalError(format!("Unable to add constant: name={name}, value={value}.")));
                    }
                }
            }
            SymbolLoader::Basic { info, supplier, protected } => {
                for (id, key, name) in info.ids()? {
                    // A missing or empty vartype means unit.
                    let sub = match info.vartype.get(key) {
                        Some(v) if !v.is_empty() => {
                            let parts = v.split(',').map(|n| types.find(n, false)).collect::<Result<Vec<_>, _>>()?;
                            types.from_list(&parts)
                        }
                        _ => UNIT,
                    };
                    let is_protected = *protected && info.protect.get(key).copied().unwrap_or(false);
                    let ty = supplier.supply(types, sub);
                    let symbol = tables.add(Symbol::Basic { name: name.to_owned(), ty, is_protected });
                    if !tables.insert(SymbolTables::ROOT, SymbolType::Basic(ty), symbol) {
                        return Err(FatalError(format!("Unable to add basic: type={}, name={name}", types[ty].representation)));
                    }
                    mapper.put_symbol(id, symbol);
                }
            }
            SymbolLoader::Event => {
                for &(name, offset, ty) in EVENTS {
                    let symbol = tables.add(Symbol::Basic { name: name.to_owned(), ty, is_protected: false });
                    if !tables.insert(SymbolTables::ROOT, SymbolType::Basic(ty), symbol) {
                        return Err(FatalError(format!("Unable to add event: {name}")));
                    }
                    mapper.put_symbol(i32::MIN + offset, symbol);
                }
                let symbol = tables.add(Symbol::Constant { name: "event_opbase".into(), value: "event_opbase".into() });
                if !tables.insert(SymbolTables::ROOT, SymbolType::Constant, symbol) {
                    return Err(FatalError("Unable to add event: event_opbase".into()));
                }
            }
        }
        Ok(())
    }
}
