//! Expression nodes. Each node struct carries `type`/`typeHint`, reached
//! through [`Expression`]. `reference` and call `symbol` slots dump as `null`
//! while `None`.

use super::{Loc, Token};
use crate::symbol::SymbolId;
use crate::types::TypeId;

/// An identifier (also used for every `name` field).
#[derive(Clone, PartialEq, Debug)]
pub struct Identifier {
    pub source: Loc,
    pub ty: Option<TypeId>,
    pub type_hint: Option<TypeId>,
    pub text: String,
    pub reference: Option<SymbolId>,
}

/// `ArithmeticExpression` / `ConditionExpression` (`BinaryExpression`).
#[derive(Clone, PartialEq, Debug)]
pub struct BinaryExpression {
    pub source: Loc,
    pub ty: Option<TypeId>,
    pub type_hint: Option<TypeId>,
    pub left: Box<Expression>,
    pub operator: Token,
    pub right: Box<Expression>,
}

/// `CalcExpression` / `ParenthesizedExpression`.
#[derive(Clone, PartialEq, Debug)]
pub struct WrappedExpression {
    pub source: Loc,
    pub ty: Option<TypeId>,
    pub type_hint: Option<TypeId>,
    pub expression: Box<Expression>,
}

/// `CommandCallExpression`; `arguments2` is `Some` for the `name*(..)(..)` form.
#[derive(Clone, PartialEq, Debug)]
pub struct CommandCallExpression {
    pub source: Loc,
    pub ty: Option<TypeId>,
    pub type_hint: Option<TypeId>,
    pub name: Identifier,
    pub arguments: Vec<Expression>,
    pub arguments2: Option<Vec<Expression>>,
    pub symbol: Option<SymbolId>,
    /// A client hook setter's checked hook string (see [`crate::hook`]).
    pub hook: Option<Box<Hook>>,
}

/// A typed client hook string: the `[clientscript]` call it names (a
/// [`CommandCallExpression`] whose symbol is the script) and its trigger names
/// with the var/inv/stat symbol each resolved to.
#[derive(Clone, PartialEq, Debug)]
pub struct Hook {
    pub call: Expression,
    pub triggers: Vec<(String, Option<SymbolId>)>,
}

/// `ProcCallExpression` / `JumpCallExpression`.
#[derive(Clone, PartialEq, Debug)]
pub struct CallExpression {
    pub source: Loc,
    pub ty: Option<TypeId>,
    pub type_hint: Option<TypeId>,
    pub name: Identifier,
    pub arguments: Vec<Expression>,
    pub symbol: Option<SymbolId>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum StringPart {
    /// `BasicStringPart`: unescaped text, or a tag's raw text.
    Basic { source: Loc, value: String },
    /// `PTagStringPart`.
    PTag { source: Loc, value: String },
    /// `ExpressionStringPart`.
    Expression { source: Loc, expression: Expression },
}

#[derive(Clone, PartialEq, Debug)]
pub struct JoinedStringExpression {
    pub source: Loc,
    pub ty: Option<TypeId>,
    pub type_hint: Option<TypeId>,
    pub parts: Vec<StringPart>,
}

/// `Literal<T>` for boolean/coord/null/character.
#[derive(Clone, PartialEq, Debug)]
pub struct Literal<T> {
    pub source: Loc,
    pub ty: Option<TypeId>,
    pub type_hint: Option<TypeId>,
    pub value: T,
    pub reference: Option<SymbolId>,
}

/// `IntegerLiteral`: `value` is the digits as written (decimal keeps its `-`,
/// hex/bin drop the `0x`/`0b` prefix).
#[derive(Clone, PartialEq, Debug)]
pub struct IntegerLiteral {
    pub source: Loc,
    pub ty: Option<TypeId>,
    pub type_hint: Option<TypeId>,
    pub value: String,
    pub radix: u32,
    pub reference: Option<SymbolId>,
    pub number_value: Option<NumberValue>,
}

/// An integer literal's value: `i32`, or `i64` for `long`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NumberValue {
    Int(i32),
    Long(i64),
}

#[derive(Clone, PartialEq, Debug)]
pub struct StringLiteral {
    pub source: Loc,
    pub ty: Option<TypeId>,
    pub type_hint: Option<TypeId>,
    pub value: String,
    pub sub_expression: Option<Box<Expression>>,
    pub reference: Option<SymbolId>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct ConstantVariableExpression {
    pub source: Loc,
    pub ty: Option<TypeId>,
    pub type_hint: Option<TypeId>,
    pub name: Identifier,
    pub sub_expression: Option<Box<Expression>>,
    pub reference: Option<SymbolId>,
}

/// `GameVariableExpression`; `dot` is the `.%` form.
#[derive(Clone, PartialEq, Debug)]
pub struct GameVariableExpression {
    pub source: Loc,
    pub ty: Option<TypeId>,
    pub type_hint: Option<TypeId>,
    pub dot: bool,
    pub name: Identifier,
    pub reference: Option<SymbolId>,
}

/// `LocalVariableExpression`; `index` is `Some` for `$name(index)`.
#[derive(Clone, PartialEq, Debug)]
pub struct LocalVariableExpression {
    pub source: Loc,
    pub ty: Option<TypeId>,
    pub type_hint: Option<TypeId>,
    pub name: Identifier,
    pub index: Option<Box<Expression>>,
    pub reference: Option<SymbolId>,
}

macro_rules! expressions {
    ($($variant:ident($node:ty),)*) => {
        #[derive(Clone, PartialEq, Debug)]
        pub enum Expression {
            $($variant($node),)*
        }

        impl Expression {
            pub fn source(&self) -> &Loc {
                match self { $(Expression::$variant(e) => &e.source,)* }
            }

            pub fn ty(&self) -> Option<TypeId> {
                match self { $(Expression::$variant(e) => e.ty,)* }
            }

            pub fn type_hint(&self) -> Option<TypeId> {
                match self { $(Expression::$variant(e) => e.type_hint,)* }
            }

            pub fn set_ty(&mut self, ty: Option<TypeId>) {
                match self { $(Expression::$variant(e) => e.ty = ty,)* }
            }

            pub fn set_type_hint(&mut self, ty: Option<TypeId>) {
                match self { $(Expression::$variant(e) => e.type_hint = ty,)* }
            }
        }
    };
}

expressions! {
    Arithmetic(BinaryExpression),
    Condition(BinaryExpression),
    Calc(WrappedExpression),
    Parenthesized(WrappedExpression),
    CommandCall(CommandCallExpression),
    ProcCall(CallExpression),
    JumpCall(CallExpression),
    Identifier(Identifier),
    JoinedString(JoinedStringExpression),
    BooleanLiteral(Literal<bool>),
    CharacterLiteral(Literal<char>),
    CoordLiteral(Literal<i32>),
    IntegerLiteral(IntegerLiteral),
    NullLiteral(Literal<i32>),
    StringLiteral(StringLiteral),
    ConstantVariable(ConstantVariableExpression),
    GameVariable(GameVariableExpression),
    LocalVariable(LocalVariableExpression),
}
