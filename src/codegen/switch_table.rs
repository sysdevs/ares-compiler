//! Switch tables. Cases stay in source order and are never sorted.
//! `K` is the key type (codegen uses [`super::ir::SwitchKey`]).

use super::label::Label;

#[derive(Clone, PartialEq, Debug)]
pub struct SwitchCase<K> {
    pub label: Label,
    pub keys: Vec<K>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct SwitchTable<K> {
    pub id: usize,
    pub cases: Vec<SwitchCase<K>>,
}

impl<K> SwitchTable<K> {
    pub fn new(id: usize) -> Self {
        Self { id, cases: Vec::new() }
    }
}
