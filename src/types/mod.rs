//! The type system.
//!
//! Every type lives in the [`TypeManager`] arena: identity is [`TypeId`] equality
//! and the class hierarchy is [`TypeKind`]. Never compare [`TypeData`]
//! structurally where identity is meant; supplier loaders create a fresh type
//! per symbol on purpose.
//!
//! The static types (primitive, meta and script var types) are seeded at fixed
//! ids by [`TypeManager::new`]. Each manager has its own copy, so
//! `change_options("long", ..)` never leaks across compilations.

mod manager;

pub use manager::{TypeChecker, TypeManager};

use std::fmt;

use crate::trigger::TriggerId;

/// Identity of a type: index into the [`TypeManager`] arena.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TypeId(pub(crate) u32);

/// Which stack a type lives on.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BaseVarType {
    Integer = 0,
    Long = 1,
    String = 2,
}

/// A type's default value, for the types that have one.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DefaultValue {
    Int(i32),
    String(&'static str),
    Long(i64),
}

/// Per-type options; every flag defaults to `true`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TypeOptions {
    pub allow_switch: bool,
    pub allow_array: bool,
    pub allow_declaration: bool,
    pub allow_parameter: bool,
}

impl Default for TypeOptions {
    fn default() -> Self {
        Self { allow_switch: true, allow_array: true, allow_declaration: true, allow_parameter: true }
    }
}

impl TypeOptions {
    pub(crate) const NONE: Self =
        Self { allow_switch: false, allow_array: false, allow_declaration: false, allow_parameter: false };
}

/// Which kind of type this is.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TypeKind {
    Primitive,
    ScriptVar,
    /// A plain meta type (`Any`, `Nothing`, `Error`, `Unit`).
    Meta,
    MetaType { inner: TypeId },
    Script { trigger: TriggerId, parameter_type: TypeId, return_type: TypeId },
    /// Children are already flattened (never a tuple, len >= 2).
    Tuple(Vec<TypeId>),
    Array { inner: TypeId },
    VarPlayer { inner: TypeId },
    VarBit { inner: TypeId },
    VarNpc { inner: TypeId },
    VarObj { inner: TypeId },
    VarShared { inner: TypeId },
    Param { inner: TypeId },
    DbColumn { inner: TypeId },
}

impl TypeKind {
    /// The wrapped inner type, if this kind wraps one (the wrapped-type checker uses it).
    pub fn inner(&self) -> Option<TypeId> {
        match *self {
            TypeKind::MetaType { inner }
            | TypeKind::Array { inner }
            | TypeKind::VarPlayer { inner }
            | TypeKind::VarBit { inner }
            | TypeKind::VarNpc { inner }
            | TypeKind::VarObj { inner }
            | TypeKind::VarShared { inner }
            | TypeKind::Param { inner }
            | TypeKind::DbColumn { inner } => Some(inner),
            _ => None,
        }
    }

    pub fn is_meta(&self) -> bool {
        matches!(self, TypeKind::Meta | TypeKind::MetaType { .. } | TypeKind::Script { .. })
    }

    pub fn is_game_var(&self) -> bool {
        matches!(
            self,
            TypeKind::VarPlayer { .. } | TypeKind::VarBit { .. } | TypeKind::VarNpc { .. } | TypeKind::VarObj { .. } | TypeKind::VarShared { .. }
        )
    }
}

#[derive(Clone, Debug)]
pub struct TypeData {
    pub representation: String,
    /// `None` for tuples, game vars, params and db columns.
    pub base_type: Option<BaseVarType>,
    pub options: TypeOptions,
    pub kind: TypeKind,
    code: Option<u8>,
    default_value: Option<DefaultValue>,
}

impl TypeData {
    /// The type's code (ares-shared `var_type`); an error on meta types and arrays.
    pub fn code(&self) -> Result<Option<u8>, TypeError> {
        if self.kind.is_meta() {
            return Err(TypeError("MetaType has no character representation.".into()));
        }
        if let TypeKind::Array { .. } = self.kind {
            return Err(TypeError("ArrayType has no character representation.".into()));
        }
        Ok(self.code)
    }

    /// The default value; an error on arrays.
    pub fn default_value(&self) -> Result<Option<&DefaultValue>, TypeError> {
        if let TypeKind::Array { .. } = self.kind {
            return Err(TypeError("ArrayType has no default value.".into()));
        }
        Ok(self.default_value.as_ref())
    }
}

/// A type system error, carrying its message.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TypeError(pub String);

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TypeError {}

// Primitive then meta types, in seeding order. The script var types follow
// in `crate::runescript::types`.
pub const INT: TypeId = TypeId(0);
pub const BOOLEAN: TypeId = TypeId(1);
pub const COORD: TypeId = TypeId(2);
pub const STRING: TypeId = TypeId(3);
pub const CHAR: TypeId = TypeId(4);
pub const LONG: TypeId = TypeId(5);
pub const MAPZONE: TypeId = TypeId(6);
pub const ANY: TypeId = TypeId(7);
pub const NOTHING: TypeId = TypeId(8);
pub const ERROR: TypeId = TypeId(9);
pub const UNIT: TypeId = TypeId(10);

/// Every primitive type.
pub const PRIMITIVES: [TypeId; 7] = [INT, BOOLEAN, COORD, STRING, CHAR, LONG, MAPZONE];
pub(crate) const FIRST_SCRIPT_VAR: u32 = 11;

fn builtins() -> Vec<TypeData> {
    let no_array_switch = TypeOptions { allow_array: false, allow_switch: false, ..TypeOptions::default() };
    let prim = |rep: &'static str, base, default, options| TypeData {
        representation: rep.into(),
        base_type: Some(base),
        options,
        kind: TypeKind::Primitive,
        code: Some(ares_shared::var_type::bytecode(rep)),
        default_value: Some(default),
    };
    let meta = |rep: &str| TypeData {
        representation: rep.into(),
        base_type: Some(BaseVarType::Integer),
        options: TypeOptions::NONE,
        kind: TypeKind::Meta,
        code: None,
        default_value: Some(DefaultValue::Int(-1)),
    };
    use BaseVarType::*;
    use DefaultValue as D;
    let opts = TypeOptions::default();
    let mut v = vec![
        prim("int", Integer, D::Int(0), opts),
        prim("boolean", Integer, D::Int(0), opts),
        prim("coord", Integer, D::Int(-1), opts),
        prim("string", String, D::String(""), no_array_switch),
        prim("char", Integer, D::Int(-1), opts),
        prim("long", Long, D::Long(0), no_array_switch),
        prim("mapzone", Integer, D::Int(-1), opts),
        meta("any"),
        meta("nothing"),
        meta("error"),
        meta("unit"),
    ];
    v.extend(crate::runescript::types::SCRIPT_VAR_TYPES.iter().map(|&(rep, code)| TypeData {
        representation: rep.into(),
        base_type: Some(Integer),
        options: opts,
        kind: TypeKind::ScriptVar,
        code: Some(code),
        default_value: Some(D::Int(-1)),
    }));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runescript::types as sv;

    #[test]
    fn builtin_codes_and_representations() {
        let t = TypeManager::new();
        let code = |id| t[id].code().unwrap().map(u32::from);
        assert_eq!(code(INT), Some('i' as u32));
        assert_eq!(code(LONG), Some(0xCF), "'Ï' is emitted via charCodeAt");
        assert_eq!(code(sv::MAPELEMENT), Some(0xB5));
        assert_eq!(code(sv::VERIFY_OBJECT), Some(0xAE));
        assert_eq!(t[LONG].default_value().unwrap(), Some(&DefaultValue::Long(0)));
        assert_eq!(t[STRING].default_value().unwrap(), Some(&DefaultValue::String("")));
        assert!(!t[STRING].options.allow_switch && !t[LONG].options.allow_array && t[INT].options.allow_array);
        assert!(t[ANY].code().is_err());
        assert_eq!(t[UNIT].default_value().unwrap(), Some(&DefaultValue::Int(-1)));
        // Script var consts line up with the seeded table.
        for (i, &(rep, c)) in sv::SCRIPT_VAR_TYPES.iter().enumerate() {
            let id = TypeId(FIRST_SCRIPT_VAR + i as u32);
            assert_eq!((t[id].code().unwrap(), t[id].representation.as_str()), (Some(c), rep));
        }
        assert_eq!(t[sv::OBJ].representation, "obj");
        assert_eq!(t[sv::VERIFY_OBJECT].representation, "verifyobj");
    }

    #[test]
    fn wrapped_and_meta_types() {
        let mut t = TypeManager::new();
        let ty = t.meta_type(ANY);
        assert_eq!(t[ty].representation, "type");
        let ty = t.meta_type(INT);
        assert_eq!(t[ty].representation, "type<int>");
        assert!(t[ty].code().is_err());
        let p = t.param(ANY);
        let p2 = t.param(sv::OBJ);
        assert_eq!((t[p].representation.as_str(), t[p2].representation.as_str()), ("param", "param<obj>"));
        assert!(t[p].options.allow_parameter && !t[p].options.allow_declaration);
        let s = t.script(crate::runescript::triggers::PROC, UNIT, UNIT);
        assert_eq!(t[s].representation, "proc");
        assert!(t[s].options.allow_parameter && !t[s].options.allow_switch);
        let a = t.array(INT).unwrap();
        assert_eq!(t[a].representation, "intarray");
        assert!(t[a].code().is_err() && t[a].default_value().is_err());
        assert!(t[a].options.allow_switch && !t[a].options.allow_array);
        assert!(t.array(a).is_err());
        let v = t.var_player(INT);
        let v2 = t.var_player(INT);
        assert_ne!(v, v2, "supplier types are fresh instances");
        assert_eq!(t[v].representation, "varp<int>");
        assert_eq!(t[v].code().unwrap(), None);
    }

    #[test]
    fn tuples_flatten_and_list_round_trip() {
        let mut t = TypeManager::new();
        assert_eq!(t.from_list(&[]), UNIT);
        assert_eq!(t.from_list(&[INT]), INT);
        let a = t.from_list(&[INT, STRING]);
        let b = t.from_list(&[a, sv::OBJ]);
        assert_eq!(t[b].representation, "int,string,obj");
        assert_eq!(t.to_list(Some(b)), [INT, STRING, sv::OBJ]);
        assert_eq!(t.to_list(Some(NOTHING)), []);
        assert_eq!(t.to_list(None), []);
        assert!(t.tuple(&[INT]).is_err());
        assert_eq!(t[b].base_type, None);
        assert_eq!(t[b].code().unwrap(), None);
    }

    #[test]
    fn manager_register_find_and_check() {
        let mut t = TypeManager::new();
        for id in PRIMITIVES {
            t.register(id).unwrap();
        }
        t.register(ANY).unwrap();
        t.register_as("type", ANY).unwrap();
        assert_eq!(t.register(INT).unwrap_err().0, "Type 'int' is already registered.");
        assert_eq!(t.register_as("type", INT).unwrap_err().0, "Type 'type' is already registered.");
        assert_eq!(t.find("type", false).unwrap(), ANY);
        let arr = t.find("intarray", true).unwrap();
        assert_eq!(t[arr].kind, TypeKind::Array { inner: INT });
        assert_ne!(t.find("intarray", true).unwrap(), arr, "each lookup allocates");
        assert!(t.find_or_null("intarray", false).is_none());
        assert!(t.find_or_null("stringarray", true).is_none(), "string disallows arrays");
        assert_eq!(t.find("nope", false).unwrap_err().0, "Unable to find type: 'nope'.");

        t.change_options("long", |o| o.allow_declaration = false).unwrap();
        assert!(!t[LONG].options.allow_declaration);
        assert!(t.change_options("nope", |_| {}).is_err());

        assert!(!t.check(INT, INT), "no checkers, nothing assignable");
        t.add_type_checker(Box::new(|_, l, r| l == r));
        t.add_type_checker(Box::new(|m, l, r| match (&m[l].kind, &m[r].kind) {
            (TypeKind::Tuple(a), TypeKind::Tuple(b)) => a.len() == b.len() && a.iter().zip(b).all(|(&x, &y)| m.check(x, y)),
            _ => false,
        }));
        let x = t.from_list(&[INT, STRING]);
        let y = t.from_list(&[INT, STRING]);
        assert!(t.check(INT, INT) && !t.check(INT, STRING) && t.check(x, y));
    }
}
