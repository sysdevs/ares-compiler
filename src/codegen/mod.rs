//! Code generation: typed AST → [`ir::RuneScript`]s.
//!
//! Which instructions carry a `source` decides the line-number table, so which
//! `instruction(…, source)` call sites pass one is fixed and reaches output.
//! No `LineNumber` instruction is ever emitted. An unsupported or impossible
//! node (including an unimplemented one) is a [`FatalError`].

pub mod ir;
pub mod label;
pub mod pointer;
pub mod switch_table;

use crate::ast::expr::*;
use crate::ast::stmt::Statement;
use crate::ast::{Loc, Script, ScriptFile};
use crate::diagnostics::{messages as msg, Diagnostics};
use crate::runescript::compiler::ServerScriptCompiler;
use crate::symbol::{Symbol, SymbolId};
use crate::trigger::COMMAND;
use crate::types::{BaseVarType, DefaultValue, TypeId, INT, STRING};
use crate::FatalError;
use ir::{Block, Instruction, Opcode, Operand, RuneScript, SwitchKey};
use label::{Label, LabelGenerator};
use switch_table::{SwitchCase, SwitchTable};

type R<T = ()> = Result<T, FatalError>;

fn fatal<T>(message: impl Into<String>) -> R<T> {
    Err(FatalError(message.into()))
}

/// `ScriptCompiler.codegen`: one generator per file, scripts in file order.
pub fn generate(c: &ServerScriptCompiler, files: &[ScriptFile], diagnostics: &mut Diagnostics) -> R<Vec<RuneScript>> {
    let mut scripts = Vec::new();
    for file in files {
        let mut g = CodeGenerator { c, d: diagnostics, labels: LabelGenerator::default(), scripts: Vec::new(), block: 0, script_header: (String::new(), String::new()) };
        for script in &file.scripts {
            g.script(script)?;
        }
        scripts.extend(g.scripts);
    }
    Ok(scripts)
}

pub struct CodeGenerator<'a> {
    c: &'a ServerScriptCompiler,
    d: &'a mut Diagnostics,
    labels: LabelGenerator,
    scripts: Vec<RuneScript>,
    /// Index of the bound block in the current script.
    block: usize,
    /// The current script's trigger and name text (for the `script` command).
    script_header: (String, String),
}

impl CodeGenerator<'_> {
    fn script_mut(&mut self) -> &mut RuneScript {
        self.scripts.last_mut().expect("inside a script")
    }

    /// `instruction(opcode, operand, source)` into the bound block.
    pub fn instruction(&mut self, opcode: Opcode, operand: Operand, source: Option<&Loc>) {
        let block = self.block;
        self.script_mut().blocks[block].instructions.push(Instruction { opcode, operand, source: source.cloned() });
    }

    /// Opens a new block for `label` and makes it current.
    fn bind_new(&mut self, label: Label) {
        let blocks = &mut self.script_mut().blocks;
        blocks.push(Block { label, instructions: Vec::new() });
        self.block = blocks.len() - 1;
    }

    fn type_rep(&self, t: TypeId) -> &str {
        &self.c.types[t].representation
    }

    fn script(&mut self, script: &Script) -> R {
        if script.trigger_type == Some(COMMAND) {
            return Ok(());
        }
        let symbol = script.symbol.expect("registered scripts reach codegen");
        let Symbol::Script { trigger, name, .. } = &self.c.tables[symbol] else { unreachable!("script symbol") };
        let (trigger, name) = (*trigger, name.clone());
        let params: Vec<SymbolId> = script.parameters.iter().flatten().map(|p| p.symbol.expect("registered")).collect();
        self.scripts.push(RuneScript {
            source_name: script.source.name.clone(),
            symbol,
            trigger,
            full_name: format!("[{},{name}]", trigger.get().identifier),
            name,
            subject_reference: script.subject_reference,
            locals: params.clone(),
            parameters: params,
            blocks: Vec::new(),
            switch_tables: Vec::new(),
        });
        self.script_header = (script.trigger.text.clone(), script.name.text.clone());
        self.bind_new(Label { name: "entry".into() });
        for s in &script.statements {
            self.statement(s)?;
        }
        // Default returns.
        for t in self.c.types.to_list(script.return_type) {
            let data = &self.c.types[t];
            let operand = match (t, data.base_type) {
                (INT, _) => (Opcode::PushConstantInt, Operand::Int(0)),
                (_, Some(BaseVarType::Integer)) => (Opcode::PushConstantInt, Operand::Int(-1)),
                (_, Some(BaseVarType::String)) => (Opcode::PushConstantString, Operand::String(String::new())),
                (_, Some(BaseVarType::Long)) => (Opcode::PushConstantLong, Operand::Long(-1)),
                (_, None) => return fatal(format!("Unsupported type in returns: {}", data.representation)),
            };
            self.instruction(operand.0, operand.1, None);
        }
        self.instruction(Opcode::Return, Operand::None, None);
        self.labels.reset();
        Ok(())
    }

    fn statement(&mut self, s: &Statement) -> R {
        match s {
            Statement::Block { statements, .. } => statements.iter().try_for_each(|s| self.statement(s)),
            Statement::Return { source, expressions } => {
                expressions.iter().try_for_each(|e| self.expr(e))?;
                self.instruction(Opcode::Return, Operand::None, Some(source));
                Ok(())
            }
            Statement::If { condition, then_statement, else_statement, .. } => {
                let if_true = self.labels.generate("if_true");
                let if_else = else_statement.as_ref().map(|_| self.labels.generate("if_else"));
                let if_end = self.labels.generate("if_end");
                self.condition(condition, if_true.clone(), if_else.clone().unwrap_or_else(|| if_end.clone()))?;
                self.bind_new(if_true);
                self.statement(then_statement)?;
                self.instruction(Opcode::Branch, Operand::Label(if_end.clone()), None);
                if let (Some(label), Some(else_statement)) = (if_else, else_statement) {
                    self.bind_new(label);
                    self.statement(else_statement)?;
                    self.instruction(Opcode::Branch, Operand::Label(if_end.clone()), None);
                }
                self.bind_new(if_end);
                Ok(())
            }
            Statement::While { condition, then_statement, .. } => {
                let start = self.labels.generate("while_start");
                let body = self.labels.generate("while_body");
                let end = self.labels.generate("while_end");
                self.bind_new(start.clone());
                self.condition(condition, body.clone(), end.clone())?;
                self.bind_new(body);
                self.statement(then_statement)?;
                self.instruction(Opcode::Branch, Operand::Label(start), None);
                self.bind_new(end);
                Ok(())
            }
            Statement::Switch { source, condition, cases, default_case, .. } => {
                let id = self.script_mut().switch_tables.len();
                self.script_mut().switch_tables.push(SwitchTable::new(id));
                let default = default_case.map(|_| self.labels.generate("switch_default_case"));
                let end = self.labels.generate("switch_end");
                self.expr(condition)?;
                self.instruction(Opcode::Switch, Operand::Switch(id), Some(source));
                if cases.first().is_none_or(|c| !c.keys.is_empty()) {
                    let target = default.clone().unwrap_or_else(|| end.clone());
                    self.instruction(Opcode::Branch, Operand::Label(target), None);
                }
                for case in cases {
                    let label = if case.keys.is_empty() {
                        default.clone().expect("a default case implies a default label")
                    } else {
                        self.labels.generate(&format!("switch_{id}_case"))
                    };
                    let mut keys = Vec::new();
                    for key in &case.keys {
                        match self.constant_value(key) {
                            Some(k) => keys.push(k),
                            None => {
                                let name = node_name(key);
                                self.d.error(key.source(), msg::NULL_CONSTANT, vec![name.into()]);
                            }
                        }
                    }
                    self.script_mut().switch_tables[id].cases.push(SwitchCase { label: label.clone(), keys });
                    self.bind_new(label);
                    case.statements.iter().try_for_each(|s| self.statement(s))?;
                    self.instruction(Opcode::Branch, Operand::Label(end.clone()), None);
                }
                self.bind_new(end);
                Ok(())
            }
            Statement::Declaration { source, initializer, symbol, .. } => {
                let symbol = symbol.expect("type checked");
                self.script_mut().locals.push(symbol);
                match initializer {
                    Some(init) => self.expr(init)?,
                    None => {
                        let Symbol::LocalVariable { ty, .. } = self.c.tables[symbol] else { unreachable!("local") };
                        let (opcode, operand) = match self.c.types[ty].default_value()? {
                            Some(DefaultValue::Int(n)) => (Opcode::PushConstantInt, Operand::Int(*n)),
                            Some(DefaultValue::String(s)) => (Opcode::PushConstantString, Operand::String((*s).into())),
                            Some(DefaultValue::Long(n)) => (Opcode::PushConstantLong, Operand::Long(*n)),
                            None => return fatal(format!("Type {} has no default value.", self.c.types[ty].representation)),
                        };
                        self.instruction(opcode, operand, None);
                    }
                }
                self.instruction(Opcode::PopLocalVar, Operand::Symbol(symbol), Some(source));
                Ok(())
            }
            Statement::ArrayDeclaration { source, initializer, symbol, .. } => {
                let symbol = symbol.expect("type checked");
                self.script_mut().locals.push(symbol);
                self.expr(initializer)?;
                self.instruction(Opcode::DefineArray, Operand::Symbol(symbol), Some(source));
                Ok(())
            }
            Statement::Assignment { vars, expressions, .. } => {
                if let Some(Expression::LocalVariable(v)) = vars.first()
                    && let Some(index) = &v.index
                {
                    self.expr(index)?;
                }
                expressions.iter().try_for_each(|e| self.expr(e))?;
                for var in vars.iter().rev() {
                    let (reference, dot) = match var {
                        Expression::LocalVariable(v) => (v.reference, None),
                        Expression::GameVariable(g) => (g.reference, Some(g.dot)),
                        _ => unreachable!("assignable"),
                    };
                    let Some(reference) = reference else {
                        self.d.error(var.source(), msg::SYMBOL_IS_NULL, vec![]);
                        return Ok(());
                    };
                    let opcode = match (&self.c.tables[reference], dot) {
                        (Symbol::LocalVariable { .. }, _) => Opcode::PopLocalVar,
                        (Symbol::Basic { .. }, Some(false)) => Opcode::PopVar,
                        (Symbol::Basic { .. }, Some(true)) => Opcode::PopVar2,
                        (Symbol::Basic { .. }, None) => return fatal("Expected 'GameVariableExpression'."),
                        (other, _) => return fatal(format!("Unsupported reference type: {}.", other.name())),
                    };
                    self.instruction(opcode, Operand::Symbol(reference), Some(var.source()));
                }
                Ok(())
            }
            Statement::Expression { source, expression } => {
                self.expr(expression)?;
                for t in self.c.types.to_list(expression.ty()) {
                    let Some(base) = self.c.types[t].base_type else {
                        self.d.error(source, msg::TYPE_HAS_NO_BASETYPE, vec![self.c.types[t].representation.clone()]);
                        return Ok(());
                    };
                    self.instruction(Opcode::Discard, Operand::BaseVar(base), None);
                }
                Ok(())
            }
            Statement::Empty { .. } => Ok(()),
        }
    }

    /// Branches go into the bound block.
    fn condition(&mut self, condition: &Expression, branch_true: Label, branch_false: Label) -> R {
        match condition {
            Expression::Condition(b) | Expression::Arithmetic(b) => {
                let op = b.operator.text.as_str();
                if op != "&" && op != "|" {
                    let left_type = b.left.ty().expect("type checked");
                    let Some(base) = self.c.types[left_type].base_type else {
                        self.d.error(b.left.source(), msg::TYPE_HAS_NO_BASETYPE, vec![self.c.types[left_type].representation.clone()]);
                        return Ok(());
                    };
                    let opcode = match (base, op) {
                        (BaseVarType::Integer, "=") => Opcode::BranchEquals,
                        (BaseVarType::Integer, "!") => Opcode::BranchNot,
                        (BaseVarType::Integer, "<") => Opcode::BranchLessThan,
                        (BaseVarType::Integer, ">") => Opcode::BranchGreaterThan,
                        (BaseVarType::Integer, "<=") => Opcode::BranchLessThanOrEquals,
                        (BaseVarType::Integer, ">=") => Opcode::BranchGreaterThanOrEquals,
                        (BaseVarType::String, "=") => Opcode::ObjBranchEquals,
                        (BaseVarType::String, "!") => Opcode::ObjBranchNot,
                        (BaseVarType::Long, "=") => Opcode::LongBranchEquals,
                        (BaseVarType::Long, "!") => Opcode::LongBranchNot,
                        (BaseVarType::Long, "<") => Opcode::LongBranchLessThan,
                        (BaseVarType::Long, ">") => Opcode::LongBranchGreaterThan,
                        (BaseVarType::Long, "<=") => Opcode::LongBranchLessThanOrEquals,
                        (BaseVarType::Long, ">=") => Opcode::LongBranchGreaterThanOrEquals,
                        _ => return fatal(format!("No mappings for operator: {op}")),
                    };
                    self.expr(&b.left)?;
                    self.expr(&b.right)?;
                    self.instruction(opcode, Operand::Label(branch_true), None);
                    self.instruction(Opcode::Branch, Operand::Label(branch_false), None);
                } else {
                    let or = op == "|";
                    let next = self.labels.generate(if or { "condition_or" } else { "condition_and" });
                    let (t, f) = if or { (branch_true.clone(), next.clone()) } else { (next.clone(), branch_false.clone()) };
                    self.condition(&b.left, t, f)?;
                    self.bind_new(next);
                    self.condition(&b.right, branch_true, branch_false)?;
                }
                Ok(())
            }
            Expression::Parenthesized(w) => self.condition(&w.expression, branch_true, branch_false),
            other => {
                self.d.error(other.source(), msg::INVALID_CONDITION, vec![node_name(other).into()]);
                Ok(())
            }
        }
    }

    fn constant_value(&self, e: &Expression) -> Option<SwitchKey> {
        match e {
            Expression::ConstantVariable(c) => c.sub_expression.as_deref().and_then(|s| self.constant_value(s)),
            Expression::Identifier(i) => i.reference.map(SwitchKey::Symbol),
            Expression::IntegerLiteral(l) => l.reference.map(SwitchKey::Symbol).or(match l.number_value {
                Some(NumberValue::Int(n)) => Some(SwitchKey::Int(n)),
                Some(NumberValue::Long(n)) => Some(SwitchKey::Long(n)),
                None => None,
            }),
            Expression::BooleanLiteral(l) => Some(l.reference.map_or(SwitchKey::Bool(l.value), SwitchKey::Symbol)),
            Expression::CharacterLiteral(l) => Some(l.reference.map_or(SwitchKey::String(l.value.to_string()), SwitchKey::Symbol)),
            Expression::CoordLiteral(l) => Some(l.reference.map_or(SwitchKey::Int(l.value), SwitchKey::Symbol)),
            Expression::NullLiteral(l) => Some(l.reference.map_or(SwitchKey::Int(l.value), SwitchKey::Symbol)),
            Expression::StringLiteral(l) => Some(l.reference.map_or(SwitchKey::String(l.value.clone()), SwitchKey::Symbol)),
            _ => None,
        }
    }

    pub(crate) fn expr(&mut self, e: &Expression) -> R {
        match e {
            Expression::Parenthesized(w) | Expression::Calc(w) => self.expr(&w.expression),
            Expression::Condition(_) => fatal("Not implemented: ConditionExpression"),
            Expression::Arithmetic(b) => {
                let left_type = b.left.ty().expect("type checked");
                let long = match self.c.types[left_type].base_type {
                    Some(BaseVarType::Integer) => false,
                    Some(BaseVarType::Long) => true,
                    other => return fatal(format!("No mapping for BaseType: {other:?}.")),
                };
                let opcode = match (b.operator.text.as_str(), long) {
                    ("+", false) => Opcode::Add,
                    ("-", false) => Opcode::Sub,
                    ("*", false) => Opcode::Multiply,
                    ("/", false) => Opcode::Divide,
                    ("%", false) => Opcode::Modulo,
                    ("&", false) => Opcode::And,
                    ("|", false) => Opcode::Or,
                    ("+", true) => Opcode::LongAdd,
                    ("-", true) => Opcode::LongSub,
                    ("*", true) => Opcode::LongMultiply,
                    ("/", true) => Opcode::LongDivide,
                    ("%", true) => Opcode::LongModulo,
                    ("&", true) => Opcode::LongAnd,
                    ("|", true) => Opcode::LongOr,
                    (op, _) => return fatal(format!("No mapping for operator: {op}.")),
                };
                self.expr(&b.left)?;
                self.expr(&b.right)?;
                self.instruction(opcode, Operand::None, None);
                Ok(())
            }
            Expression::CommandCall(c) => {
                let Some(symbol) = c.symbol else {
                    self.d.error(&c.source, msg::SYMBOL_IS_NULL, vec![]);
                    return Ok(());
                };
                let name = if c.arguments2.is_some() { format!("{}*", c.name.text) } else { c.name.text.clone() };
                if self.dynamic_command(&name, e)? {
                    return Ok(());
                }
                c.arguments.iter().try_for_each(|a| self.expr(a))?;
                self.instruction(Opcode::Command, Operand::Symbol(symbol), Some(&c.source));
                Ok(())
            }
            Expression::ProcCall(c) | Expression::JumpCall(c) => {
                let Some(symbol) = c.symbol else {
                    self.d.error(&c.source, msg::SYMBOL_IS_NULL, vec![]);
                    return Ok(());
                };
                c.arguments.iter().try_for_each(|a| self.expr(a))?;
                let opcode = if matches!(e, Expression::ProcCall(_)) { Opcode::Gosub } else { Opcode::Jump };
                self.instruction(opcode, Operand::Symbol(symbol), Some(&c.source));
                Ok(())
            }
            Expression::LocalVariable(v) => {
                let Some(reference) = v.reference else {
                    self.d.error(&v.source, msg::SYMBOL_IS_NULL, vec![]);
                    return Ok(());
                };
                if let Some(index) = &v.index {
                    self.expr(index)?;
                }
                self.instruction(Opcode::PushLocalVar, Operand::Symbol(reference), Some(&v.source));
                Ok(())
            }
            Expression::GameVariable(g) => {
                let Some(reference) = g.reference else {
                    self.d.error(&g.source, msg::SYMBOL_IS_NULL, vec![]);
                    return Ok(());
                };
                let opcode = if g.dot { Opcode::PushVar2 } else { Opcode::PushVar };
                self.instruction(opcode, Operand::Symbol(reference), Some(&g.source));
                Ok(())
            }
            Expression::ConstantVariable(c) => match &c.sub_expression {
                Some(sub) => self.expr(sub),
                None => {
                    self.d.error(&c.source, msg::EXPRESSION_NO_SUBEXPR, vec![]);
                    Ok(())
                }
            },
            Expression::IntegerLiteral(l) => {
                if let Some(r) = l.reference {
                    self.instruction(Opcode::PushConstantSymbol, Operand::Symbol(r), Some(&l.source));
                } else if l.ty == Some(STRING) {
                    self.instruction(Opcode::PushConstantString, Operand::String(l.value.clone()), Some(&l.source));
                } else {
                    match l.number_value {
                        Some(NumberValue::Int(n)) => self.instruction(Opcode::PushConstantInt, Operand::Int(n), Some(&l.source)),
                        Some(NumberValue::Long(n)) => self.instruction(Opcode::PushConstantLong, Operand::Long(n), Some(&l.source)),
                        None => return fatal(format!("Integer literal has no numeric value: {}", l.value)),
                    }
                }
                Ok(())
            }
            Expression::CoordLiteral(l) => {
                self.instruction(Opcode::PushConstantInt, Operand::Int(l.value), Some(&l.source));
                Ok(())
            }
            Expression::BooleanLiteral(l) => {
                if l.ty == Some(STRING) {
                    self.instruction(Opcode::PushConstantString, Operand::String(l.value.to_string()), Some(&l.source));
                } else {
                    self.instruction(Opcode::PushConstantInt, Operand::Int(l.value as i32), Some(&l.source));
                }
                Ok(())
            }
            Expression::CharacterLiteral(l) => {
                // Its code point (the parser checked it is one char).
                self.instruction(Opcode::PushConstantInt, Operand::Int(l.value as i32), Some(&l.source));
                Ok(())
            }
            Expression::NullLiteral(l) => {
                let base = l.ty.and_then(|t| self.c.types[t].base_type);
                match base {
                    Some(BaseVarType::String) => self.instruction(Opcode::PushConstantString, Operand::String("null".into()), None),
                    Some(BaseVarType::Long) => self.instruction(Opcode::PushConstantLong, Operand::Long(-1), None),
                    // The hook hack after this is unreachable (no hook types).
                    _ => self.instruction(Opcode::PushConstantInt, Operand::Int(-1), Some(&l.source)),
                }
                Ok(())
            }
            Expression::StringLiteral(l) => {
                if let Some(sub) = &l.sub_expression {
                    return self.expr(sub);
                }
                match l.reference {
                    Some(r) => self.instruction(Opcode::PushConstantSymbol, Operand::Symbol(r), Some(&l.source)),
                    None => self.instruction(Opcode::PushConstantString, Operand::String(l.value.clone()), Some(&l.source)),
                }
                Ok(())
            }
            Expression::JoinedString(j) => {
                for part in &j.parts {
                    match part {
                        StringPart::Basic { source, value } | StringPart::PTag { source, value } => {
                            self.instruction(Opcode::PushConstantString, Operand::String(value.clone()), Some(source))
                        }
                        StringPart::Expression { expression, .. } => self.expr(expression)?,
                    }
                }
                if j.parts.len() > 1 {
                    self.instruction(Opcode::JoinString, Operand::Int(j.parts.len() as i32), Some(&j.source));
                }
                Ok(())
            }
            Expression::Identifier(i) => {
                let Some(reference) = i.reference else {
                    if i.ty == Some(STRING) {
                        self.instruction(Opcode::PushConstantString, Operand::String(i.text.clone()), Some(&i.source));
                    } else {
                        self.d.error(&i.source, msg::SYMBOL_IS_NULL, vec![]);
                    }
                    return Ok(());
                };
                if let Symbol::Script { trigger: COMMAND, .. } = self.c.tables[reference] {
                    if self.dynamic_command(&i.text, e)? {
                        return Ok(());
                    }
                    self.instruction(Opcode::Command, Operand::Symbol(reference), Some(&i.source));
                } else {
                    self.instruction(Opcode::PushConstantSymbol, Operand::Symbol(reference), Some(&i.source));
                }
                Ok(())
            }
        }
    }

    fn dynamic_command(&mut self, name: &str, e: &Expression) -> R<bool> {
        let Some(&handler) = self.c.dynamic_commands.get(name) else { return Ok(false) };
        handler.generate_code(&mut CodeGeneratorContext { generator: self, expression: e })?;
        Ok(true)
    }
}

/// `constructor.name` of an expression node (diagnostic arguments).
fn node_name(e: &Expression) -> &'static str {
    match e {
        Expression::Arithmetic(_) => "ArithmeticExpression",
        Expression::Condition(_) => "ConditionExpression",
        Expression::Calc(_) => "CalcExpression",
        Expression::Parenthesized(_) => "ParenthesizedExpression",
        Expression::CommandCall(_) => "CommandCallExpression",
        Expression::ProcCall(_) => "ProcCallExpression",
        Expression::JumpCall(_) => "JumpCallExpression",
        Expression::Identifier(_) => "Identifier",
        Expression::JoinedString(_) => "JoinedStringExpression",
        Expression::BooleanLiteral(_) => "BooleanLiteral",
        Expression::CharacterLiteral(_) => "CharacterLiteral",
        Expression::CoordLiteral(_) => "CoordLiteral",
        Expression::IntegerLiteral(_) => "IntegerLiteral",
        Expression::NullLiteral(_) => "NullLiteral",
        Expression::StringLiteral(_) => "StringLiteral",
        Expression::ConstantVariable(_) => "ConstantVariableExpression",
        Expression::GameVariable(_) => "GameVariableExpression",
        Expression::LocalVariable(_) => "LocalVariableExpression",
    }
}

/// `CodeGeneratorContext`: what a dynamic command handler sees.
pub struct CodeGeneratorContext<'g, 'a> {
    generator: &'g mut CodeGenerator<'a>,
    pub expression: &'g Expression,
}

impl<'g, 'a> CodeGeneratorContext<'g, 'a> {
    pub fn compiler(&self) -> &'a ServerScriptCompiler {
        self.generator.c
    }

    /// `arguments` (empty for an identifier).
    pub fn arguments(&self) -> &'g [Expression] {
        match self.expression {
            Expression::CommandCall(c) => &c.arguments,
            _ => &[],
        }
    }

    /// `arguments2`: only for `name*(…)(…)`.
    pub fn arguments2(&self) -> &'g [Expression] {
        match self.expression {
            Expression::CommandCall(c) => c.arguments2.as_deref().unwrap_or_default(),
            _ => &[],
        }
    }

    pub fn instruction(&mut self, opcode: Opcode, operand: Operand) {
        self.generator.instruction(opcode, operand, None);
    }

    pub fn visit(&mut self, e: &Expression) -> R {
        self.generator.expr(e)
    }

    pub fn visit_all(&mut self, es: &[Expression]) -> R {
        es.iter().try_for_each(|e| self.generator.expr(e))
    }

    /// `command()`: the call's `Command` instruction, with the expression's source.
    pub fn command(&mut self) -> R {
        let symbol = match self.expression {
            Expression::CommandCall(c) => c.symbol,
            Expression::Identifier(i) => i.reference,
            _ => None,
        };
        let Some(symbol) = symbol else { return fatal("Symbol cannot be null for command generation.") };
        self.generator.instruction(Opcode::Command, Operand::Symbol(symbol), Some(self.expression.source()));
        Ok(())
    }

    /// The current script's `[trigger, name]` texts (for the `script` command).
    pub fn script_header(&self) -> &(String, String) {
        &self.generator.script_header
    }

    pub fn type_code(&self, t: Option<TypeId>) -> R<Option<u8>> {
        let Some(t) = t else { return fatal("Internal compiler error: expression has no type.") };
        Ok(self.generator.c.types[t].code()?)
    }

    pub fn type_rep(&self, t: TypeId) -> &str {
        self.generator.type_rep(t)
    }

    /// The base type of a wrapped type's inner type.
    pub fn inner_base_type(&self, t: Option<TypeId>) -> Option<BaseVarType> {
        let inner = t.and_then(|t| self.generator.c.types[t].kind.inner())?;
        self.generator.c.types[inner].base_type
    }
}
