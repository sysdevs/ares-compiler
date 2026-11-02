//! The AST as plain data: no parent pointers, no visitor.
//! Passes walk it with `match`; [`dump`] renders the golden `ast.json` shape.

pub mod dump;
pub mod expr;
pub mod location;
pub mod stmt;

pub use location::NodeSourceLocation as Loc;

use expr::Identifier;
use stmt::Statement;

use crate::symbol::{ScopeId, SymbolId};
use crate::trigger::TriggerId;
use crate::types::TypeId;

/// `Token` node: a raw grammar token kept in the tree (types, operators).
#[derive(Clone, PartialEq, Debug)]
pub struct Token {
    pub source: Loc,
    pub text: String,
}

#[derive(Clone, PartialEq, Debug)]
pub struct ScriptFile {
    pub source: Loc,
    pub scripts: Vec<Script>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Script {
    pub source: Loc,
    pub trigger: Identifier,
    /// A multi-word name becomes one identifier with the words joined by a space.
    pub name: Identifier,
    pub is_star: bool,
    /// `None` when the script has no `(…)` parameter group at all.
    pub parameters: Option<Vec<Parameter>>,
    /// `None` when the script has no second `(…)` group.
    pub return_tokens: Option<Vec<Token>>,
    pub statements: Vec<Statement>,
    /// Set by script registration (`None` until then) from here on.
    /// `None` when the trigger name is unknown.
    pub trigger_type: Option<TriggerId>,
    pub parameter_type: Option<TypeId>,
    pub return_type: Option<TypeId>,
    /// `None` when the script was not inserted (unknown/disabled trigger, redeclaration).
    pub symbol: Option<SymbolId>,
    pub subject_reference: Option<SymbolId>,
    /// The script's own symbol table.
    pub block: Option<ScopeId>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Parameter {
    pub source: Loc,
    pub type_token: Token,
    pub name: Identifier,
    /// Set by script registration.
    pub symbol: Option<SymbolId>,
}
