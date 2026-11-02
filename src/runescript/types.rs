//! The script var types. Every entry is INTEGER-based, default `-1`, all options on.
//! (Param and db column types are [`TypeKind`](crate::types::TypeKind) variants.)

use crate::types::{FIRST_SCRIPT_VAR, TypeId};

/// Every script var type as `(representation, code)`, in declaration order.
pub use ares_shared::var_type::SCRIPT as SCRIPT_VAR_TYPES;

/// One `TypeId` per [`SCRIPT_VAR_TYPES`] entry (the list lives in ares-shared).
macro_rules! type_ids {
    ($($name:ident $rep:literal = $code:literal,)*) => {
        $(pub const $name: TypeId = TypeId(FIRST_SCRIPT_VAR + ares_shared::var_type::script_index($rep) as u32);)*
    };
}
ares_shared::script_var_types!(type_ids);
