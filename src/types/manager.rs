//! The type registry, plus the constructors of every non-static type kind.

use std::collections::HashMap;
use std::ops::Index;

use crate::trigger::TriggerId;

use super::{
    builtins, BaseVarType, DefaultValue, TypeData, TypeError, TypeId, TypeKind, TypeOptions, ANY, NOTHING,
    UNIT,
};

/// `(left, right) => boolean`; gets the manager so checkers can recurse via [`TypeManager::check`].
pub type TypeChecker = Box<dyn Fn(&TypeManager, TypeId, TypeId) -> bool + Send + Sync>;

pub struct TypeManager {
    types: Vec<TypeData>,
    name_to_type: HashMap<String, TypeId>,
    checkers: Vec<TypeChecker>,
}

impl Default for TypeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Index<TypeId> for TypeManager {
    type Output = TypeData;
    fn index(&self, id: TypeId) -> &TypeData {
        &self.types[id.0 as usize]
    }
}

impl TypeManager {
    /// Arena seeded with the static types; nothing is registered by name yet.
    pub fn new() -> Self {
        Self { types: builtins(), name_to_type: HashMap::new(), checkers: Vec::new() }
    }

    fn alloc(&mut self, representation: String, kind: TypeKind) -> TypeId {
        // Every allocated (non-static) class has INTEGER base and -1 default except
        // tuples (no base type, no default); options and code are fixed per class.
        let (base_type, default_value, options) = match kind {
            TypeKind::Tuple(_) => (None, None, TypeOptions::NONE),
            TypeKind::Script { .. } => (
                Some(BaseVarType::Integer),
                Some(DefaultValue::Int(-1)),
                TypeOptions { allow_parameter: true, ..TypeOptions::NONE },
            ),
            TypeKind::Array { .. } => (
                Some(BaseVarType::Integer),
                None,
                TypeOptions { allow_array: false, ..TypeOptions::default() },
            ),
            TypeKind::Param { .. } | TypeKind::DbColumn { .. } => (
                Some(BaseVarType::Integer),
                Some(DefaultValue::Int(-1)),
                TypeOptions { allow_parameter: true, ..TypeOptions::NONE },
            ),
            _ => (Some(BaseVarType::Integer), Some(DefaultValue::Int(-1)), TypeOptions::NONE),
        };
        let id = TypeId(self.types.len() as u32);
        self.types.push(TypeData { representation, base_type, options, kind, code: None, default_value });
        id
    }

    fn rep(&self, id: TypeId) -> &str {
        &self[id].representation
    }

    pub fn meta_type(&mut self, inner: TypeId) -> TypeId {
        let rep = if inner == ANY { "type".to_owned() } else { format!("type<{}>", self.rep(inner)) };
        self.alloc(rep, TypeKind::MetaType { inner })
    }

    pub fn script(&mut self, trigger: TriggerId, parameter_type: TypeId, return_type: TypeId) -> TypeId {
        self.alloc(trigger.get().identifier.to_owned(), TypeKind::Script { trigger, parameter_type, return_type })
    }

    /// Flattens one level; an error below two children.
    pub fn tuple(&mut self, children: &[TypeId]) -> Result<TypeId, TypeError> {
        let mut flat = Vec::with_capacity(children.len());
        for &c in children {
            match &self[c].kind {
                TypeKind::Tuple(inner) => flat.extend_from_slice(inner),
                _ => flat.push(c),
            }
        }
        if flat.len() < 2 {
            return Err(TypeError("TupleType should not be used when type count is < 2".into()));
        }
        let rep = flat.iter().map(|&c| self.rep(c)).collect::<Vec<_>>().join(",");
        Ok(self.alloc(rep, TypeKind::Tuple(flat)))
    }

    /// `unit` for none, the type itself for one, else a tuple.
    pub fn from_list(&mut self, types: &[TypeId]) -> TypeId {
        match types {
            [] => UNIT,
            [t] => *t,
            _ => self.tuple(types).expect(">= 2 types never flatten below 2"),
        }
    }

    /// The inverse of [`Self::from_list`]; a missing type, `unit` and `nothing` give none.
    pub fn to_list(&self, ty: Option<TypeId>) -> Vec<TypeId> {
        match ty {
            None | Some(UNIT) | Some(NOTHING) => vec![],
            Some(t) => match &self[t].kind {
                TypeKind::Tuple(children) => children.clone(),
                _ => vec![t],
            },
        }
    }

    pub fn array(&mut self, inner: TypeId) -> Result<TypeId, TypeError> {
        if let TypeKind::Array { .. } = self[inner].kind {
            return Err(TypeError("ArrayType cannot wrap another ArrayType.".into()));
        }
        let rep = format!("{}array", self.rep(inner));
        Ok(self.alloc(rep, TypeKind::Array { inner }))
    }

    pub fn var_player(&mut self, inner: TypeId) -> TypeId {
        let rep = format!("varp<{}>", self.rep(inner));
        self.alloc(rep, TypeKind::VarPlayer { inner })
    }

    pub fn var_bit(&mut self, inner: TypeId) -> TypeId {
        let rep = format!("varbit<{}>", self.rep(inner));
        self.alloc(rep, TypeKind::VarBit { inner })
    }

    pub fn var_npc(&mut self, inner: TypeId) -> TypeId {
        let rep = format!("varn<{}>", self.rep(inner));
        self.alloc(rep, TypeKind::VarNpc { inner })
    }

    pub fn var_obj(&mut self, inner: TypeId) -> TypeId {
        let rep = format!("varo<{}>", self.rep(inner));
        self.alloc(rep, TypeKind::VarObj { inner })
    }

    pub fn var_client(&mut self, inner: TypeId) -> TypeId {
        let rep = format!("varc<{}>", self.rep(inner));
        self.alloc(rep, TypeKind::VarClient { inner })
    }

    pub fn var_shared(&mut self, inner: TypeId) -> TypeId {
        let rep = format!("vars<{}>", self.rep(inner));
        self.alloc(rep, TypeKind::VarShared { inner })
    }

    pub fn param(&mut self, inner: TypeId) -> TypeId {
        let rep = if inner == ANY { "param".to_owned() } else { format!("param<{}>", self.rep(inner)) };
        self.alloc(rep, TypeKind::Param { inner })
    }

    pub fn db_column(&mut self, inner: TypeId) -> TypeId {
        let rep = format!("dbcolumn<{}>", self.rep(inner));
        self.alloc(rep, TypeKind::DbColumn { inner })
    }

    /// Registers under the type's representation; a repeated name is an error.
    pub fn register(&mut self, ty: TypeId) -> Result<(), TypeError> {
        let name = self[ty].representation.clone();
        if self.name_to_type.contains_key(&name) {
            return Err(TypeError(format!("Type '{name}' is already registered.")));
        }
        self.name_to_type.insert(name, ty);
        Ok(())
    }

    /// Registers under `name`; a repeated name is an error.
    pub fn register_as(&mut self, name: &str, ty: TypeId) -> Result<(), TypeError> {
        if self.name_to_type.contains_key(name) {
            return Err(TypeError(format!("Type '{name}' is already registered.")));
        }
        self.name_to_type.insert(name.to_owned(), ty);
        Ok(())
    }

    pub fn change_options(&mut self, name: &str, builder: impl FnOnce(&mut TypeOptions)) -> Result<(), TypeError> {
        let id = *self.name_to_type.get(name).ok_or_else(|| TypeError(format!("'{name}' was not found.")))?;
        builder(&mut self.types[id.0 as usize].options);
        Ok(())
    }

    pub fn find(&mut self, name: &str, allow_array: bool) -> Result<TypeId, TypeError> {
        self.find_or_null(name, allow_array).ok_or_else(|| TypeError(format!("Unable to find type: '{name}'.")))
    }

    /// An `…array` lookup allocates a fresh array type.
    pub fn find_or_null(&mut self, name: &str, allow_array: bool) -> Option<TypeId> {
        if allow_array && let Some(base_name) = name.strip_suffix("array") {
            let base = self.find_or_null(base_name, false)?;
            if !self[base].options.allow_array {
                return None;
            }
            // Registered types are never arrays (only `find` creates them).
            return Some(self.array(base).expect("registered type is an ArrayType"));
        }
        self.name_to_type.get(name).copied()
    }

    /// Every registered `(name, type)`, in no particular order.
    pub fn names(&self) -> impl Iterator<Item = (&str, TypeId)> {
        self.name_to_type.iter().map(|(k, v)| (k.as_str(), *v))
    }

    pub fn add_type_checker(&mut self, checker: TypeChecker) {
        self.checkers.push(checker);
    }

    /// Is `right` assignable to `left`.
    pub fn check(&self, left: TypeId, right: TypeId) -> bool {
        self.checkers.iter().any(|c| c(self, left, right))
    }
}
