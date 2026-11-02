//! Pointer sets per script; `PointerType` lives in `ares_shared::pointer`.

pub use ares_shared::pointer::PointerType;
use indexmap::IndexSet;

/// Sets keep insertion order, hence `IndexSet`.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PointerHolder {
    pub required: IndexSet<PointerType>,
    pub set: IndexSet<PointerType>,
    pub conditional_set: bool,
    pub corrupted: IndexSet<PointerType>,
}
