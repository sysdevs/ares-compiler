//! Statement nodes.

use super::expr::{Expression, Identifier};
use super::{Loc, Token};
use crate::symbol::SymbolId;
use crate::types::TypeId;

#[derive(Clone, PartialEq, Debug)]
pub struct SwitchCase {
    pub source: Loc,
    /// Empty for `case default`.
    pub keys: Vec<Expression>,
    pub statements: Vec<Statement>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Statement {
    Block { source: Loc, statements: Vec<Statement> },
    Return { source: Loc, expressions: Vec<Expression> },
    If { source: Loc, condition: Expression, then_statement: Box<Statement>, else_statement: Option<Box<Statement>> },
    While { source: Loc, condition: Expression, then_statement: Box<Statement> },
    /// `ty`: the switch type, once type checking resolved it.
    /// `default_case` indexes `cases`.
    Switch {
        source: Loc,
        type_token: Token,
        condition: Expression,
        cases: Vec<SwitchCase>,
        ty: Option<TypeId>,
        default_case: Option<usize>,
    },
    Declaration { source: Loc, type_token: Token, name: Identifier, initializer: Option<Expression>, symbol: Option<SymbolId> },
    ArrayDeclaration { source: Loc, type_token: Token, name: Identifier, initializer: Expression, symbol: Option<SymbolId> },
    /// `vars` are local or game variable expressions.
    Assignment { source: Loc, vars: Vec<Expression>, expressions: Vec<Expression> },
    Expression { source: Loc, expression: Expression },
    Empty { source: Loc },
}

impl Statement {
    pub fn source(&self) -> &Loc {
        match self {
            Statement::Block { source, .. }
            | Statement::Return { source, .. }
            | Statement::If { source, .. }
            | Statement::While { source, .. }
            | Statement::Switch { source, .. }
            | Statement::Declaration { source, .. }
            | Statement::ArrayDeclaration { source, .. }
            | Statement::Assignment { source, .. }
            | Statement::Expression { source, .. }
            | Statement::Empty { source } => source,
        }
    }
}
