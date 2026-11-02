//! The code generator's IR and its `ir.json` rendering.

use serde_json::{json, Value};

use super::label::Label;
use super::switch_table::SwitchTable;
use crate::ast::Loc;
use crate::symbol::{Symbol, SymbolId, SymbolTables};
use crate::trigger::TriggerId;
use crate::types::{BaseVarType, TypeManager};

/// Variant names are the opcode names in `ir.json`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Opcode {
    PushConstantInt,
    PushConstantString,
    PushConstantLong,
    PushConstantSymbol,
    PushLocalVar,
    PopLocalVar,
    PushVar,
    PushVar2,
    PopVar,
    PopVar2,
    DefineArray,
    Switch,
    Branch,
    BranchNot,
    BranchEquals,
    BranchLessThan,
    BranchGreaterThan,
    BranchLessThanOrEquals,
    BranchGreaterThanOrEquals,
    LongBranchNot,
    LongBranchEquals,
    LongBranchLessThan,
    LongBranchGreaterThan,
    LongBranchLessThanOrEquals,
    LongBranchGreaterThanOrEquals,
    ObjBranchNot,
    ObjBranchEquals,
    JoinString,
    Discard,
    Gosub,
    Jump,
    Command,
    Return,
    Add,
    Sub,
    Multiply,
    Divide,
    Modulo,
    Or,
    And,
    LongAdd,
    LongSub,
    LongMultiply,
    LongDivide,
    LongModulo,
    LongOr,
    LongAnd,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Operand {
    /// No operand (unit opcodes).
    None,
    Int(i32),
    String(String),
    Long(i64),
    Symbol(SymbolId),
    Label(Label),
    /// A switch table id.
    Switch(usize),
    /// `Discard`'s stack.
    BaseVar(BaseVarType),
}

/// `source` set means the instruction feeds the line-number table.
#[derive(Clone, PartialEq, Debug)]
pub struct Instruction {
    pub opcode: Opcode,
    pub operand: Operand,
    pub source: Option<Loc>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Block {
    pub label: Label,
    pub instructions: Vec<Instruction>,
}

/// A resolved switch key: whatever the literal or reference was.
#[derive(Clone, PartialEq, Debug)]
pub enum SwitchKey {
    Symbol(SymbolId),
    Int(i32),
    Long(i64),
    Bool(bool),
    /// A character literal's value is its one-character string.
    String(String),
}

#[derive(Clone, PartialEq, Debug)]
pub struct RuneScript {
    pub source_name: String,
    pub symbol: SymbolId,
    pub trigger: TriggerId,
    pub name: String,
    /// `[trigger,name]`.
    pub full_name: String,
    pub subject_reference: Option<SymbolId>,
    pub parameters: Vec<SymbolId>,
    /// `locals.all`: parameters, then declarations in visit order.
    pub locals: Vec<SymbolId>,
    pub blocks: Vec<Block>,
    pub switch_tables: Vec<SwitchTable<SwitchKey>>,
}

/// The `ir.json` rendering of one script.
pub fn dump(script: &RuneScript, types: &TypeManager, tables: &SymbolTables) -> Value {
    let symbol = |s: SymbolId| -> Value {
        let rep = |t| types[t].representation.clone();
        match &tables[s] {
            Symbol::LocalVariable { name, ty } => json!({ "$sym": "LocalVariableSymbol", "name": name, "type": rep(*ty) }),
            Symbol::Basic { name, ty, is_protected } => json!({ "$sym": "BasicSymbol", "name": name, "type": rep(*ty), "isProtected": is_protected }),
            Symbol::Constant { name, value } => json!({ "$sym": "ConstantSymbol", "name": name, "value": value }),
            Symbol::Script { trigger, name, parameters, returns } => json!({
                "$sym": "ServerScriptSymbol",
                "trigger": trigger.get().identifier, "name": name, "parameters": rep(*parameters), "returns": rep(*returns),
            }),
        }
    };
    let operand = |o: &Operand| match o {
        Operand::None => Value::Null,
        Operand::Int(n) => json!(n),
        Operand::String(s) => json!(s),
        Operand::Long(n) => json!(format!("{n}n")),
        Operand::Symbol(s) => symbol(*s),
        Operand::Label(l) => json!({ "$label": l.name }),
        Operand::Switch(id) => json!({ "$switch": id }),
        // Written as its number.
        Operand::BaseVar(b) => json!(*b as i32),
    };
    let key = |k: &SwitchKey| match k {
        SwitchKey::Symbol(s) => symbol(*s),
        SwitchKey::Int(n) => json!(n),
        SwitchKey::Long(n) => json!(format!("{n}n")),
        SwitchKey::Bool(b) => json!(b),
        SwitchKey::String(s) => json!(s),
    };
    json!({
        "fullName": script.full_name,
        "sourceName": script.source_name,
        "subjectReference": script.subject_reference.map_or(Value::Null, symbol),
        "locals": {
            "parameters": script.parameters.iter().map(|&s| symbol(s)).collect::<Vec<_>>(),
            "all": script.locals.iter().map(|&s| symbol(s)).collect::<Vec<_>>(),
        },
        "blocks": script.blocks.iter().map(|b| json!({
            "label": b.label.name,
            "instructions": b.instructions.iter().map(|i| json!({
                "op": format!("{:?}", i.opcode), "operand": operand(&i.operand), "line": i.source.as_ref().map(|s| s.line),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "switchTables": script.switch_tables.iter().map(|t| json!({
            "id": t.id,
            "cases": t.cases.iter().map(|c| json!({ "label": c.label.name, "keys": c.keys.iter().map(key).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}
