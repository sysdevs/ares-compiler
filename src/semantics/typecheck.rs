//! Type checking: resolves every expression's
//! type and symbol after script registration.
//!
//! A plain recursive walk; what it needs from enclosing nodes is state carried
//! down the walk (the current script's trigger and return type, the enclosing
//! switch type, whether a statement sits at the top level of its script). An
//! internal error (including a missing type reaching the type checkers) aborts
//! the compilation and is returned as [`FatalError`].

use indexmap::IndexSet;

use crate::ast::expr::*;
use crate::ast::stmt::{Statement, SwitchCase};
use crate::ast::{Loc, Script, ScriptFile};
use crate::diagnostics::{args, closest, messages as msg, DiagnosticType, Diagnostics};
use crate::parser::parse_single_expression;
use crate::runescript::compiler::ServerScriptCompiler;
use crate::runescript::{triggers, types as sv};
use crate::symbol::{ScopeId, Symbol, SymbolId, SymbolTables, SymbolType};
use crate::trigger::{TriggerId, COMMAND};
use crate::types::{TypeId, TypeKind, TypeManager, ANY, BOOLEAN, CHAR, COORD, ERROR, INT, LONG, STRING, UNIT};
use crate::FatalError;

type R<T = ()> = Result<T, FatalError>;

const LITERAL_TYPES: [TypeId; 6] = [INT, BOOLEAN, COORD, STRING, CHAR, LONG];

/// Type checks every script of `files` (after registration).
pub fn type_check(c: &mut ServerScriptCompiler, files: &mut [ScriptFile], diagnostics: &mut Diagnostics) -> R {
    let mut t = Checker {
        c,
        d: diagnostics,
        table: SymbolTables::ROOT,
        evaluating: IndexSet::new(),
        script_trigger: None,
        script_return: None,
    };
    for file in files {
        for script in &mut file.scripts {
            t.script(script)?;
        }
    }
    Ok(())
}

pub struct Checker<'a> {
    c: &'a mut ServerScriptCompiler,
    d: &'a mut Diagnostics,
    table: ScopeId,
    /// Constants being evaluated, in insertion order.
    evaluating: IndexSet<SymbolId>,
    script_trigger: Option<TriggerId>,
    script_return: Option<TypeId>,
}

impl Checker<'_> {
    fn error(&mut self, source: &Loc, message: &str, a: Vec<String>) {
        self.d.error(source, message, a);
    }

    fn rep(&self, t: TypeId) -> String {
        self.c.types[t].representation.clone()
    }

    fn scoped<T>(&mut self, scope: ScopeId, f: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        let old = std::mem::replace(&mut self.table, scope);
        let r = f(self);
        self.table = old;
        r
    }

    fn script(&mut self, script: &mut Script) -> R {
        self.script_trigger = script.trigger_type;
        self.script_return = script.return_type;
        let block = script.block.expect("registration sets the script table");
        self.scoped(block, |t| script.statements.iter_mut().try_for_each(|s| t.statement(s, true)))
    }

    // ------------------------------------------------------------------ statements

    fn statement(&mut self, s: &mut Statement, top_level: bool) -> R {
        match s {
            Statement::Block { statements, .. } => {
                let scope = self.c.tables.sub_table(self.table);
                self.scoped(scope, |t| statements.iter_mut().try_for_each(|s| t.statement(s, false)))
            }
            Statement::Return { source, expressions } => {
                let expected = self.c.types.to_list(self.script_return);
                let actual = self.hint_list(&expected, expressions)?;
                let (e, a) = (self.c.types.from_list(&expected), self.c.types.from_list(&actual));
                self.check_type_match(source, Some(e), Some(a), true)?;
                Ok(())
            }
            Statement::If { condition, then_statement, else_statement, .. } => {
                self.check_condition(condition)?;
                self.statement(then_statement, false)?;
                if let Some(e) = else_statement {
                    self.statement(e, false)?;
                }
                Ok(())
            }
            Statement::While { condition, then_statement, .. } => {
                self.check_condition(condition)?;
                self.statement(then_statement, false)
            }
            Statement::Switch { type_token, condition, cases, ty, default_case, .. } => {
                let type_name = type_token.text.replacen("switch_", "", 1);
                let switch_type = self.c.types.find_or_null(&type_name, false);
                match switch_type {
                    None => self.error(&type_token.source, msg::GENERIC_INVALID_TYPE, args(&[&type_name])),
                    Some(t) if !self.c.types[t].options.allow_switch => {
                        let rep = self.rep(t);
                        self.error(&type_token.source, msg::SWITCH_INVALID_TYPE, vec![rep]);
                    }
                    _ => {}
                }
                *ty = switch_type;
                condition.set_type_hint(switch_type);
                self.expr(condition)?;
                let actual = condition.ty().unwrap_or(ERROR);
                self.check_type_match(condition.source(), switch_type, Some(actual), true)?;
                let mut default = None;
                for (i, case) in cases.iter_mut().enumerate() {
                    if case.keys.is_empty() {
                        if default.is_none() {
                            default = Some(i);
                        } else {
                            self.error(&case.source, msg::SWITCH_DUPLICATE_DEFAULT, vec![]);
                        }
                    }
                    self.switch_case(case, switch_type)?;
                }
                *default_case = default;
                Ok(())
            }
            Statement::Declaration { source, type_token, name, initializer, symbol } => {
                let Some(ty) = self.declared_type(source, &type_token.text, &type_token.source, top_level, false) else {
                    return Ok(());
                };
                let sym = self.declare(&name.text, &name.source, ty);
                if let Some(init) = initializer {
                    init.set_type_hint(Some(ty));
                    self.expr(init)?;
                    let actual = init.ty().unwrap_or(ERROR);
                    self.check_type_match(init.source(), Some(ty), Some(actual), true)?;
                }
                *symbol = Some(sym);
                Ok(())
            }
            Statement::ArrayDeclaration { source, type_token, name, initializer, symbol } => {
                let Some(ty) = self.declared_type(source, &type_token.text, &type_token.source, top_level, true) else {
                    return Ok(());
                };
                initializer.set_type_hint(Some(INT));
                self.expr(initializer)?;
                let actual = initializer.ty().unwrap_or(ERROR);
                self.check_type_match(initializer.source(), Some(INT), Some(actual), true)?;
                *symbol = Some(self.declare(&name.text, &name.source, ty));
                Ok(())
            }
            Statement::Assignment { source, vars, expressions } => {
                for v in vars.iter_mut() {
                    self.expr(v)?;
                }
                let left: Vec<TypeId> = vars.iter().map(|v| v.ty().unwrap_or(ERROR)).collect();
                let right = self.hint_list(&left, expressions)?;
                let (l, r) = (self.c.types.from_list(&left), self.c.types.from_list(&right));
                self.check_type_match(source, Some(l), Some(r), true)?;
                let first_array = vars.iter().find(|v| matches!(v, Expression::LocalVariable(l) if l.index.is_some()));
                if vars.len() > 1
                    && let Some(v) = first_array
                {
                    let src = v.source().clone();
                    self.error(&src, msg::ASSIGN_MULTI_ARRAY, vec![]);
                }
                Ok(())
            }
            Statement::Expression { source, expression } => {
                self.expr(expression)?;
                if let Some(t) = expression.ty()
                    && t != ERROR
                    && !self.has_side_effects(Some(expression))
                {
                    self.d.report(DiagnosticType::Warning, source, msg::EXPRESSION_STATEMENT_NO_SIDE_EFFECT, vec![]);
                }
                Ok(())
            }
            Statement::Empty { .. } => Ok(()),
        }
    }

    /// The shared head of plain and array declarations:
    /// feature checks (`None` = stop), type lookup and its diagnostics. Arrays
    /// wrap the type (`error` when it was not found).
    fn declared_type(&mut self, source: &Loc, token: &str, token_source: &Loc, top_level: bool, array: bool) -> Option<TypeId> {
        let f = self.c.features;
        if !f.procs {
            self.error(source, msg::FEATURE_DISABLED_LOCAL, vec![]);
            return None;
        }
        if f.top_level_def_only && !top_level {
            self.error(source, msg::LOCAL_DECLARATION_NOT_TOPLEVEL, vec![]);
            return None;
        }
        let type_name = token.strip_prefix("def_").unwrap_or(token);
        let mut ty = self.c.types.find_or_null(type_name, false);
        if f.is_disabled_type_name(type_name) {
            self.error(token_source, msg::FEATURE_DISABLED_TYPE, args(&[type_name]));
            ty = Some(ERROR);
        } else if let Some(t) = ty {
            let o = self.c.types[t].options;
            if t != ERROR && !o.allow_declaration {
                let rep = self.rep(t);
                self.error(token_source, msg::LOCAL_DECLARATION_INVALID_TYPE, vec![rep]);
            } else if array && t != ERROR && !o.allow_array {
                let rep = self.rep(t);
                self.error(token_source, msg::LOCAL_ARRAY_INVALID_TYPE, vec![rep]);
            }
        } else {
            self.error(token_source, msg::GENERIC_INVALID_TYPE, args(&[type_name]));
        }
        Some(match (array, ty) {
            (true, Some(t)) => self.c.types.array(t).expect("registered types are never arrays"),
            (_, t) => t.unwrap_or(ERROR),
        })
    }

    fn declare(&mut self, name: &str, source: &Loc, ty: TypeId) -> SymbolId {
        let symbol = self.c.tables.add(Symbol::LocalVariable { name: name.to_owned(), ty });
        if self.c.tables.insert(self.table, SymbolType::LocalVariable, symbol) {
            self.c.tables.declared_at(symbol, source);
        } else {
            let first = self.c.tables.find(self.table, SymbolType::LocalVariable, name).and_then(|s| self.c.tables.decl(s)).cloned();
            self.d.emit(DiagnosticType::Error, source, msg::SCRIPT_LOCAL_REDECLARATION, args(&[name])).note(first.as_ref(), "first defined here");
        }
        symbol
    }

    fn switch_case(&mut self, case: &mut SwitchCase, switch_type: Option<TypeId>) -> R {
        for key in &mut case.keys {
            key.set_type_hint(switch_type);
            self.expr(key)?;
            if !self.is_constant(key) {
                let src = key.source().clone();
                self.error(&src, msg::SWITCH_CASE_NOT_CONSTANT, vec![]);
                continue;
            }
            self.check_type_match(key.source(), switch_type, key.ty(), true)?;
        }
        let scope = self.c.tables.sub_table(self.table);
        self.scoped(scope, |t| case.statements.iter_mut().try_for_each(|s| t.statement(s, false)))
    }

    fn check_condition(&mut self, e: &mut Expression) -> R {
        e.set_type_hint(Some(BOOLEAN));
        match find_invalid_condition(e) {
            Some(src) => self.error(&src, msg::CONDITION_INVALID_NODE_TYPE, vec![]),
            None => {
                self.expr(e)?;
                let actual = e.ty().unwrap_or(ERROR);
                self.check_type_match(e.source(), Some(BOOLEAN), Some(actual), true)?;
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------ expressions

    pub(crate) fn expr(&mut self, e: &mut Expression) -> R {
        if matches!(e, Expression::CommandCall(_)) {
            return self.command_call(e);
        }
        if matches!(e, Expression::Identifier(_)) {
            return self.identifier(e);
        }
        match e {
            Expression::Parenthesized(w) => {
                w.expression.set_type_hint(w.type_hint);
                self.expr(&mut w.expression)?;
                w.ty = w.expression.ty();
            }
            Expression::Condition(b) => {
                let ok = self.condition_operation(b)?;
                b.ty = Some(if ok { BOOLEAN } else { ERROR });
            }
            Expression::Arithmetic(b) => self.arithmetic(b)?,
            Expression::Calc(w) => {
                if !self.c.features.calc {
                    self.error(&w.source, msg::FEATURE_DISABLED_CALC, vec![]);
                    w.ty = Some(ERROR);
                    return Ok(());
                }
                w.expression.set_type_hint(w.type_hint);
                self.expr(&mut w.expression)?;
                let inner = w.expression.ty();
                if inner.is_none() || !self.match_any(&[INT, LONG], inner.unwrap())? {
                    let rep = inner.map_or("<null>".to_owned(), |t| self.rep(t));
                    self.error(w.expression.source(), msg::ARITHMETIC_INVALID_TYPE, vec![rep]);
                    w.ty = Some(ERROR);
                } else {
                    w.ty = inner;
                }
            }
            Expression::ProcCall(c) => {
                if !self.c.features.procs {
                    self.error(&c.source, msg::FEATURE_DISABLED_TRIGGER, args(&["proc"]));
                    c.ty = Some(ERROR);
                    return Ok(());
                }
                (c.symbol, c.ty) = self.check_call(triggers::PROC, msg::PROC_REFERENCE_UNRESOLVED, msg::PROC_NOARGS_EXPECTED, &c.source, &c.name.text, &mut c.arguments)?;
            }
            Expression::JumpCall(c) => {
                if self.script_trigger == Some(triggers::PROC) {
                    self.error(&c.source, "cannot jump to a label from inside a proc", vec![]);
                    return Ok(());
                }
                (c.symbol, c.ty) = self.check_call(triggers::LABEL, msg::JUMP_REFERENCE_UNRESOLVED, msg::JUMP_NOARGS_EXPECTED, &c.source, &c.name.text, &mut c.arguments)?;
            }
            Expression::LocalVariable(v) => self.local_variable(v)?,
            Expression::GameVariable(g) => {
                let symbol = self.c.tables.find_all(SymbolTables::ROOT, &g.name.text).into_iter().find_map(|s| match &self.c.tables[s] {
                    Symbol::Basic { ty, .. } if self.c.types[*ty].kind.is_game_var() => Some((s, *ty)),
                    _ => None,
                });
                match symbol {
                    None => {
                        g.ty = Some(ERROR);
                        let name = g.name.text.clone();
                        self.error(&g.source, msg::GAME_REFERENCE_UNRESOLVED, vec![name.clone()]);
                        let (tables, types) = (&self.c.tables, &self.c.types);
                        let vars = tables
                            .buckets(SymbolTables::ROOT)
                            .filter(|(ty, _)| matches!(ty, SymbolType::Basic(t) if types[*t].kind.is_game_var()))
                            .flat_map(|(_, entries)| entries.map(|(k, _)| k));
                        if let (Some(c), Some(d)) = (closest(&name.to_lowercase(), vars), self.d.list.last_mut()) {
                            d.help(format!("a game variable with a similar name exists: `%{c}`"));
                        }
                    }
                    Some((s, ty)) => {
                        g.reference = Some(s);
                        g.ty = self.c.types[ty].kind.inner();
                    }
                }
            }
            Expression::ConstantVariable(cv) => self.constant_variable(cv)?,
            Expression::IntegerLiteral(l) => self.integer_literal(l)?,
            Expression::CoordLiteral(l) => l.ty = Some(COORD),
            Expression::BooleanLiteral(l) => {
                if !self.c.features.booleans {
                    self.error(&l.source, msg::FEATURE_DISABLED_BOOLEAN, vec![]);
                    l.ty = Some(ERROR);
                    return Ok(());
                }
                // Deliberately tests `ty`, not `type_hint`, here.
                l.ty = Some(if l.ty == Some(STRING) { STRING } else { BOOLEAN });
            }
            Expression::CharacterLiteral(l) => l.ty = Some(CHAR),
            Expression::NullLiteral(l) => match l.type_hint {
                Some(h) => l.ty = Some(h),
                None => {
                    l.ty = Some(ERROR);
                    self.error(&l.source, msg::NULL_AMBIGUOUS, vec![]);
                }
            },
            Expression::StringLiteral(l) => match l.type_hint {
                None => l.ty = Some(STRING),
                Some(h) if self.c.types.check(h, STRING) => l.ty = Some(STRING),
                Some(h) if !LITERAL_TYPES.contains(&h) => {
                    let value = l.value.clone();
                    let (symbol, ty) = self.resolve_symbol(&l.source, &value, Some(h), false)?;
                    l.reference = symbol;
                    l.ty = Some(ty);
                }
                Some(_) => l.ty = Some(STRING),
            },
            Expression::JoinedString(j) => {
                for part in &mut j.parts {
                    if let StringPart::Expression { expression, .. } = part {
                        expression.set_type_hint(Some(STRING));
                        self.expr(expression)?;
                        let actual = expression.ty().unwrap_or(ERROR);
                        self.check_type_match(expression.source(), Some(STRING), Some(actual), true)?;
                    }
                }
                j.ty = Some(STRING);
            }
            Expression::CommandCall(_) | Expression::Identifier(_) => unreachable!("dispatched above"),
        }
        Ok(())
    }

    fn condition_operation(&mut self, b: &mut BinaryExpression) -> R<bool> {
        let op = b.operator.text.clone();
        let op_src = b.operator.source.clone();
        let f = self.c.features;
        if (op == "&" && !f.logical_and) || ((op == "<=" || op == ">=") && !f.relational_equals) {
            self.error(&op_src, msg::FEATURE_DISABLED_OPERATOR, vec![op]);
            return Ok(false);
        }
        let allowed: Option<&[TypeId]> = match op.as_str() {
            "&" | "|" => Some(&[BOOLEAN]),
            "<" | ">" | "<=" | ">=" => Some(&[INT, LONG]),
            _ => None,
        };
        if op != "&" && op != "|" && (is_condition(&b.left) || is_condition(&b.right)) {
            self.error(&op_src, msg::CONDITION_NOT_VALID, vec![]);
            return Ok(false);
        }
        self.visit_hinted_pair(&mut b.left, &mut b.right)?;
        let (Some(l), Some(r)) = (b.left.ty(), b.right.ty()) else {
            let (l, r) = (self.rep_or_null(b.left.ty()), self.rep_or_null(b.right.ty()));
            self.error(&op_src, msg::BINOP_INVALID_TYPES, vec![op, l, r]);
            return Ok(false);
        };
        let (lt, rt) = (self.is_tuple(l), self.is_tuple(r));
        if lt || rt {
            if lt {
                let rep = self.rep(l);
                self.error(b.left.source(), msg::BINOP_TUPLE_TYPE, vec!["left".into(), rep]);
            }
            if rt {
                let rep = self.rep(r);
                self.error(b.right.source(), msg::BINOP_TUPLE_TYPE, vec!["right".into(), rep]);
            }
            return Ok(false);
        }
        let invalid = |t: &mut Self| {
            let (lr, rr) = (t.rep(l), t.rep(r));
            t.error(&op_src, msg::BINOP_INVALID_TYPES, vec![op.clone(), lr, rr]);
            Ok(false)
        };
        if l == UNIT || r == UNIT {
            return invalid(self);
        }
        if let Some(allowed) = allowed
            && (!self.match_any(allowed, l)? || !self.match_any(allowed, r)?)
        {
            return invalid(self);
        }
        if !self.check_type_match(b.left.source(), Some(l), Some(r), false)? || (l == STRING && r == STRING) {
            return invalid(self);
        }
        Ok(true)
    }

    fn arithmetic(&mut self, b: &mut BinaryExpression) -> R {
        match b.type_hint {
            Some(h) => {
                b.left.set_type_hint(Some(h));
                self.expr(&mut b.left)?;
                b.right.set_type_hint(Some(h));
                self.expr(&mut b.right)?;
            }
            None => self.visit_hinted_pair(&mut b.left, &mut b.right)?,
        }
        let (l, r) = (b.left.ty(), b.right.ty());
        let valid = match (l, r) {
            (Some(l), Some(r)) => {
                self.match_any(&[INT, LONG], l)? && self.match_any(&[INT, LONG], r)? && self.check_type_match(b.left.source(), Some(l), Some(r), false)?
            }
            _ => false,
        };
        if !valid {
            let (lr, rr) = (self.rep_or_null(l), self.rep_or_null(r));
            let op = b.operator.text.clone();
            self.error(&b.operator.source, msg::BINOP_INVALID_TYPES, vec![op, lr, rr]);
            b.ty = Some(ERROR);
            return Ok(());
        }
        b.ty = l;
        Ok(())
    }

    fn visit_hinted_pair(&mut self, left: &mut Expression, right: &mut Expression) -> R {
        if has_concrete_type(left) || !has_concrete_type(right) {
            self.expr(left)?;
            right.set_type_hint(left.ty());
            self.expr(right)
        } else {
            self.expr(right)?;
            left.set_type_hint(right.ty());
            self.expr(left)
        }
    }

    fn command_call(&mut self, e: &mut Expression) -> R {
        let Expression::CommandCall(c) = e else { unreachable!() };
        let name = if c.arguments2.is_some() { format!("{}*", c.name.text) } else { c.name.text.clone() };
        // Also reports a feature-disabled command.
        if self.check_dynamic_command(&name, e)? {
            return Ok(());
        }
        let Expression::CommandCall(c) = e else { unreachable!() };
        (c.symbol, c.ty) = self.check_call(COMMAND, msg::COMMAND_REFERENCE_UNRESOLVED, msg::COMMAND_NOARGS_EXPECTED, &c.source, &c.name.text, &mut c.arguments)?;
        Ok(())
    }

    /// Runs the dynamic command handler for `name`, if any.
    fn check_dynamic_command(&mut self, name: &str, e: &mut Expression) -> R<bool> {
        if self.c.features.is_disabled_command_name(name) {
            let src = e.source().clone();
            self.error(&src, msg::FEATURE_DISABLED_COMMAND, args(&[name]));
            e.set_ty(Some(ERROR));
            return Ok(true);
        }
        let Some(&handler) = self.c.dynamic_commands.get(name) else { return Ok(false) };
        handler.type_check(&mut TypeCheckingContext { checker: self, expression: e })?;
        let src = e.source().clone();
        if e.ty().is_none() {
            self.error(&src, msg::CUSTOM_HANDLER_NOTYPE, vec![]);
        }
        let needs_symbol = match e {
            Expression::Identifier(i) => i.reference.is_none(),
            Expression::CommandCall(c) => c.symbol.is_none(),
            _ => false,
        };
        if needs_symbol {
            let symbol = self.c.tables.find(SymbolTables::ROOT, SymbolType::ServerScript(COMMAND), name);
            if symbol.is_none() {
                self.error(&src, msg::CUSTOM_HANDLER_NOSYMBOL, vec![]);
            }
            match e {
                Expression::Identifier(i) => i.reference = symbol,
                Expression::CommandCall(c) => c.symbol = symbol,
                _ => {}
            }
        }
        Ok(true)
    }

    /// Returns the call's (`symbol`, `type`) to assign.
    #[allow(clippy::type_complexity)]
    fn check_call(
        &mut self,
        trigger: TriggerId,
        unresolved: &str,
        no_args: &str,
        source: &Loc,
        name: &str,
        arguments: &mut [Expression],
    ) -> R<(Option<SymbolId>, Option<TypeId>)> {
        let symbol = self.c.tables.find(SymbolTables::ROOT, SymbolType::ServerScript(trigger), name);
        let result = match symbol {
            None => {
                self.error(source, unresolved, args(&[name]));
                let (what, prefix) = match trigger {
                    triggers::PROC => ("proc", "~"),
                    triggers::LABEL => ("label", "@"),
                    _ => ("command", ""),
                };
                let tables = &self.c.tables;
                if let (Some(c), Some(d)) = (closest(name, tables.names(SymbolTables::ROOT, SymbolType::ServerScript(trigger))), self.d.list.last_mut()) {
                    d.help(format!("a {what} with a similar name exists: `{prefix}{c}`"));
                }
                (None, Some(ERROR))
            }
            Some(s) => {
                let Symbol::Script { returns, .. } = self.c.tables[s] else { unreachable!("script bucket") };
                (Some(s), Some(returns))
            }
        };
        self.check_arguments(symbol, source, arguments, name, no_args)?;
        Ok(result)
    }

    fn check_arguments(&mut self, symbol: Option<SymbolId>, source: &Loc, arguments: &mut [Expression], name: &str, no_args: &str) -> R {
        let parameters = match symbol.map(|s| &self.c.tables[s]) {
            Some(Symbol::Script { parameters, .. }) => *parameters,
            _ => ERROR,
        };
        let expected = match &self.c.types[parameters].kind {
            TypeKind::Tuple(children) => children.clone(),
            _ => vec![parameters],
        };
        let actual = self.hint_list(&expected, arguments)?;
        let (e, a) = (self.c.types.from_list(&expected), self.c.types.from_list(&actual));
        if e == UNIT && a != UNIT {
            let rep = self.rep(a);
            self.error(source, no_args, args(&[name, &rep]));
            return Ok(());
        }
        if self.check_type_match(source, Some(e), Some(a), false)? {
            return Ok(());
        }
        // One argument per parameter: point at the first one that does not match.
        let at = if arguments.len() == expected.len() && actual.len() == expected.len() {
            let mut first = None;
            for (i, (&e, &a)) in expected.iter().zip(&actual).enumerate() {
                if !self.check(Some(e), Some(a))? {
                    first = Some(i);
                    break;
                }
            }
            first
        } else {
            None
        };
        let ok = match at {
            Some(i) => self.check_type_match(&arguments[i].source().clone(), Some(expected[i]), Some(actual[i]), true)?,
            None => self.check_type_match(source, Some(e), Some(a), true)?,
        };
        if !ok {
            self.defined_here(symbol);
        }
        Ok(())
    }

    /// A `note:` on the last diagnostic pointing at where the called script is declared.
    fn defined_here(&mut self, symbol: Option<SymbolId>) {
        let Some(s) = symbol else { return };
        let (Symbol::Script { trigger, name, .. }, Some(loc)) = (&self.c.tables[s], self.c.tables.decl(s)) else { return };
        let what = trigger.get().identifier;
        let note = format!("{what} `[{what},{name}]` defined here");
        if let Some(d) = self.d.list.last_mut() {
            d.note(Some(loc), note);
        }
    }

    fn local_variable(&mut self, v: &mut LocalVariableExpression) -> R {
        if !self.c.features.procs {
            self.error(&v.source, msg::FEATURE_DISABLED_LOCAL, vec![]);
            v.ty = Some(ERROR);
            return Ok(());
        }
        let name = v.name.text.clone();
        let Some(symbol) = self.c.tables.find(self.table, SymbolType::LocalVariable, &name) else {
            self.error(&v.source, msg::LOCAL_REFERENCE_UNRESOLVED, vec![name.clone()]);
            let tables = &self.c.tables;
            if let (Some(c), Some(d)) = (closest(&name, tables.names(self.table, SymbolType::LocalVariable)), self.d.list.last_mut()) {
                d.help(format!("a local variable with a similar name exists: `${c}`"));
            }
            v.ty = Some(ERROR);
            return Ok(());
        };
        let Symbol::LocalVariable { ty, .. } = self.c.tables[symbol] else { unreachable!("local bucket") };
        let array_inner = match self.c.types[ty].kind {
            TypeKind::Array { inner } => Some(inner),
            _ => None,
        };
        let message = match (array_inner.is_some(), v.index.is_some()) {
            (false, true) => Some(msg::LOCAL_REFERENCE_NOT_ARRAY),
            (true, false) => Some(msg::LOCAL_ARRAY_REFERENCE_NOINDEX),
            _ => None,
        };
        if let Some(m) = message {
            self.error(&v.source, m, vec![name]);
            v.ty = Some(ERROR);
            return Ok(());
        }
        if array_inner.is_some()
            && let Some(index) = &mut v.index
        {
            self.expr(index)?;
            let actual = index.ty().unwrap_or(ERROR);
            self.check_type_match(index.source(), Some(INT), Some(actual), true)?;
        }
        v.reference = Some(symbol);
        v.ty = Some(array_inner.unwrap_or(ty));
        Ok(())
    }

    fn constant_variable(&mut self, cv: &mut ConstantVariableExpression) -> R {
        let name = cv.name.text.clone();
        let hint = match cv.type_hint {
            None => {
                self.error(&cv.source, msg::CONSTANT_UNKNOWN_TYPE, vec![name]);
                cv.ty = Some(ERROR);
                return Ok(());
            }
            Some(ERROR) => {
                cv.ty = Some(ERROR);
                return Ok(());
            }
            Some(h) => h,
        };
        let Some(symbol) = self.c.tables.find(SymbolTables::ROOT, SymbolType::Constant, &name) else {
            self.error(&cv.source, msg::CONSTANT_REFERENCE_UNRESOLVED, vec![name.clone()]);
            let tables = &self.c.tables;
            if let (Some(c), Some(d)) = (closest(&name, tables.names(SymbolTables::ROOT, SymbolType::Constant)), self.d.list.last_mut()) {
                d.help(format!("a constant with a similar name exists: `^{c}`"));
            }
            cv.ty = Some(ERROR);
            return Ok(());
        };
        if self.evaluating.contains(&symbol) {
            let mut stack: Vec<String> = self.evaluating.iter().map(|&s| format!("^{}", self.c.tables[s].name())).collect();
            stack.push(format!("^{}", self.c.tables[symbol].name()));
            self.error(&cv.source, msg::CONSTANT_CYCLIC_REF, vec![stack.join(" -> ")]);
            cv.ty = Some(ERROR);
            return Ok(());
        }
        self.evaluating.insert(symbol);
        let result = self.constant_value(cv, symbol, hint);
        self.evaluating.shift_remove(&symbol);
        result
    }

    /// The fallible part of checking a constant variable.
    fn constant_value(&mut self, cv: &mut ConstantVariableExpression, symbol: SymbolId, hint: TypeId) -> R {
        let Symbol::Constant { value, .. } = &self.c.tables[symbol] else { unreachable!("constant bucket") };
        let value = value.clone();
        let src = cv.source.clone();
        let parsed = if hint == STRING || hint == sv::GRAPHIC {
            Some(Expression::StringLiteral(StringLiteral {
                source: Loc { name: src.name.clone(), line: src.line - 1, column: src.column - 1, end_line: src.line - 1, end_column: src.column },
                ty: None,
                type_hint: None,
                value: value.clone(),
                sub_expression: None,
                reference: None,
            }))
        } else {
            parse_single_expression(&value, &src.name, src.line - 1, src.column - 1)?.node
        };
        let Some(mut parsed) = parsed else {
            let rep = self.rep(hint);
            self.error(&src, msg::CONSTANT_PARSE_ERROR, vec![value, rep]);
            cv.ty = Some(ERROR);
            return Ok(());
        };
        parsed.set_type_hint(Some(hint));
        self.expr(&mut parsed)?;
        if !self.is_constant(&parsed) {
            self.error(&src, msg::CONSTANT_NONCONSTANT, vec![value]);
            cv.ty = Some(ERROR);
            return Ok(());
        }
        cv.ty = parsed.ty();
        cv.sub_expression = Some(Box::new(parsed));
        Ok(())
    }

    fn integer_literal(&mut self, l: &mut IntegerLiteral) -> R {
        let hint = l.type_hint;
        if hint == Some(STRING) {
            l.ty = Some(STRING);
            return Ok(());
        }
        if hint == Some(BOOLEAN) && l.radix == 10 && (l.value == "0" || l.value == "1") {
            l.number_value = Some(NumberValue::Int(if l.value == "1" { 1 } else { 0 }));
            l.ty = Some(BOOLEAN);
            return Ok(());
        }
        if let Some(h) = hint
            && h != ERROR
            && h != UNIT
            && !LITERAL_TYPES.contains(&h)
        {
            let value = l.value.clone();
            let (symbol, ty) = self.resolve_symbol(&l.source, &value, Some(h), false)?;
            l.reference = symbol;
            l.ty = Some(ty);
            return Ok(());
        }
        let ty = if hint == Some(LONG) { LONG } else { INT };
        match parse_numeric(&l.value, l.radix, ty == LONG) {
            None => {
                let rep = self.rep(ty);
                self.error(&l.source, msg::INTEGER_VALUE_OUT_OF_RANGE, vec![rep]);
                l.ty = Some(ERROR);
            }
            Some(n) => {
                l.number_value = Some(n);
                l.ty = Some(ty);
            }
        }
        Ok(())
    }

    fn identifier(&mut self, e: &mut Expression) -> R {
        let Expression::Identifier(i) = e else { unreachable!() };
        let name = i.text.clone();
        let hint = i.type_hint;
        if self.check_dynamic_command(&name, e)? {
            return Ok(());
        }
        let Expression::Identifier(i) = e else { unreachable!() };
        let (symbol, ty) = self.resolve_symbol(&i.source, &name, hint, true)?;
        i.ty = Some(ty);
        let Some(symbol) = symbol else { return Ok(()) };
        if let Symbol::Script { trigger: COMMAND, parameters, name: sym_name, .. } = &self.c.tables[symbol] {
            let (parameters, sym_name) = (*parameters, sym_name.clone());
            if parameters != UNIT {
                let rep = self.rep(parameters);
                self.error(&i.source, msg::GENERIC_TYPE_MISMATCH, vec![rep, "<unit>".into()]);
            }
            if self.c.features.is_disabled_command_name(&sym_name) {
                self.error(&i.source, msg::FEATURE_DISABLED_COMMAND, vec![sym_name]);
                i.ty = Some(ERROR);
                return Ok(());
            }
        }
        i.reference = Some(symbol);
        Ok(())
    }

    /// The symbol (if any) and the type to give the node.
    fn resolve_symbol(&mut self, source: &Loc, name: &str, hint: Option<TypeId>, allow_to_string: bool) -> R<(Option<SymbolId>, TypeId)> {
        let mut found: Option<(SymbolId, TypeId)> = None;
        for candidate in self.c.tables.find_all(self.table, name) {
            let Some(candidate_type) = self.symbol_to_type(candidate) else { continue };
            let is_script = matches!(self.c.types[candidate_type].kind, TypeKind::Script { .. });
            match hint {
                None if is_script => continue,
                None => {
                    found = Some((candidate, candidate_type));
                    break;
                }
                Some(h) if self.c.types.check(h, candidate_type) => {
                    found = Some((candidate, candidate_type));
                    break;
                }
                Some(_) if found.is_none() => found = Some((candidate, candidate_type)),
                Some(_) => {}
            }
        }
        let is_command = found.is_some_and(|(s, _)| matches!(self.c.tables[s], Symbol::Script { trigger: COMMAND, .. }));
        if allow_to_string && hint == Some(STRING) && !is_command {
            return Ok((None, STRING));
        }
        match found {
            None => {
                self.error(source, msg::GENERIC_UNRESOLVED_SYMBOL, args(&[name]));
                Ok((None, ERROR))
            }
            Some((s, t)) => Ok((Some(s), t)),
        }
    }

    /// The type a symbol gives a node; non-command scripts get a fresh script type.
    fn symbol_to_type(&mut self, symbol: SymbolId) -> Option<TypeId> {
        match self.c.tables[symbol] {
            Symbol::Script { trigger: COMMAND, returns, .. } => Some(returns),
            Symbol::Script { trigger, parameters, returns, .. } => Some(self.c.types.script(trigger, parameters, returns)),
            Symbol::LocalVariable { ty, .. } => matches!(self.c.types[ty].kind, TypeKind::Array { .. }).then_some(ty),
            Symbol::Basic { ty, .. } => Some(ty),
            Symbol::Constant { .. } => None,
        }
    }

    // ------------------------------------------------------------------ helpers

    fn hint_list(&mut self, expected: &[TypeId], expressions: &mut [Expression]) -> R<Vec<TypeId>> {
        let mut actual = Vec::new();
        let mut counter = 0;
        for e in expressions {
            e.set_type_hint(expected.get(counter).copied());
            self.expr(e)?;
            actual.push(e.ty().unwrap_or(ERROR));
            counter += match e.ty().map(|t| &self.c.types[t].kind) {
                Some(TypeKind::Tuple(children)) => children.len(),
                _ => 1,
            };
        }
        Ok(actual)
    }

    /// [`TypeManager::check`] with possibly missing types: `any`/`error` and
    /// equal sides match, any other missing type is an internal error.
    fn check(&self, l: Option<TypeId>, r: Option<TypeId>) -> R<bool> {
        match (l, r) {
            (Some(l), Some(r)) => Ok(self.c.types.check(l, r)),
            _ if l == Some(ANY) || l == Some(ERROR) || r == Some(ERROR) || l == r => Ok(true),
            _ => Err(FatalError("Internal compiler error: type check against a missing type.".into())),
        }
    }

    /// Compares `expected` with `actual` (tuples element-wise); `None` is a missing type.
    pub(crate) fn check_type_match(&mut self, source: &Loc, expected: Option<TypeId>, actual: Option<TypeId>, report: bool) -> R<bool> {
        let flatten = |t: Option<TypeId>| match t.map(|t| &self.c.types[t].kind) {
            Some(TypeKind::Tuple(children)) => children.iter().map(|&c| Some(c)).collect(),
            _ => vec![t],
        };
        let (ef, af): (Vec<_>, Vec<_>) = (flatten(expected), flatten(actual));
        let mut matched = true;
        if expected == Some(ERROR) {
            matched = true;
        } else if ef.len() != af.len() {
            matched = false;
        } else {
            for (e, a) in ef.into_iter().zip(af) {
                matched = matched && self.check(e, a)?;
            }
        }
        if !matched && report {
            let null_rep = || FatalError("Internal compiler error: type mismatch with a missing type.".into());
            let actual_rep = match actual {
                Some(UNIT) => "<unit>".to_owned(),
                Some(t) => self.rep(t),
                None => return Err(null_rep()),
            };
            let expected_rep = self.rep(expected.ok_or_else(null_rep)?);
            self.error(source, msg::GENERIC_TYPE_MISMATCH, vec![expected_rep, actual_rep]);
        }
        Ok(matched)
    }

    /// Whether `actual` matches any of `expected` (never reports, so the source is irrelevant).
    fn match_any(&mut self, expected: &[TypeId], actual: TypeId) -> R<bool> {
        let unused = Loc { name: String::new(), line: 0, column: 0, end_line: 0, end_column: 0 };
        for &t in expected {
            if self.check_type_match(&unused, Some(t), Some(actual), false)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn rep_or_null(&self, t: Option<TypeId>) -> String {
        t.map_or("<null>".to_owned(), |t| self.rep(t))
    }

    fn is_tuple(&self, t: TypeId) -> bool {
        matches!(self.c.types[t].kind, TypeKind::Tuple(_))
    }

    pub(crate) fn is_constant(&self, e: &Expression) -> bool {
        match e {
            Expression::ConstantVariable(_) => true,
            Expression::StringLiteral(l) => l.sub_expression.as_deref().is_none_or(|s| self.is_constant(s)),
            Expression::BooleanLiteral(_)
            | Expression::CharacterLiteral(_)
            | Expression::CoordLiteral(_)
            | Expression::IntegerLiteral(_)
            | Expression::NullLiteral(_) => true,
            Expression::Identifier(i) => i.reference.is_none_or(|r| matches!(self.c.tables[r], Symbol::Basic { .. } | Symbol::Constant { .. })),
            _ => false,
        }
    }

    fn has_side_effects(&self, e: Option<&Expression>) -> bool {
        let Some(e) = e else { return false };
        match e {
            Expression::CommandCall(c) => self.command_has_side_effects(c.ty),
            Expression::ProcCall(_) | Expression::JumpCall(_) => true,
            Expression::Identifier(i) => match i.reference.map(|r| &self.c.tables[r]) {
                Some(Symbol::Script { trigger: COMMAND, returns, .. }) => self.command_has_side_effects(i.ty.or(Some(*returns))),
                _ => false,
            },
            Expression::LocalVariable(v) => self.has_side_effects(v.index.as_deref()),
            Expression::ConstantVariable(c) => self.has_side_effects(c.sub_expression.as_deref()),
            Expression::Parenthesized(w) | Expression::Calc(w) => self.has_side_effects(Some(&w.expression)),
            Expression::Arithmetic(b) | Expression::Condition(b) => self.has_side_effects(Some(&b.left)) || self.has_side_effects(Some(&b.right)),
            Expression::JoinedString(j) => j.parts.iter().any(|p| matches!(p, StringPart::Expression { expression, .. } if self.has_side_effects(Some(expression)))),
            _ => false,
        }
    }

    fn command_has_side_effects(&self, t: Option<TypeId>) -> bool {
        match t {
            None | Some(ERROR) | Some(UNIT) => true,
            Some(t) => match &self.c.types[t].kind {
                TypeKind::Tuple(children) => children.iter().all(|&c| c == UNIT),
                _ => false,
            },
        }
    }
}

/// The first node that is neither a
/// condition nor parenthesised (only `&`/`|` recurse).
fn find_invalid_condition(e: &Expression) -> Option<Loc> {
    match e {
        Expression::Condition(b) if b.operator.text == "|" || b.operator.text == "&" => {
            find_invalid_condition(&b.left).or_else(|| find_invalid_condition(&b.right))
        }
        Expression::Condition(_) => None,
        Expression::Parenthesized(w) => find_invalid_condition(&w.expression),
        other => Some(other.source().clone()),
    }
}

fn is_condition(e: &Expression) -> bool {
    match e {
        Expression::Parenthesized(w) => is_condition(&w.expression),
        Expression::Condition(_) => true,
        _ => false,
    }
}

fn has_concrete_type(e: &Expression) -> bool {
    match e {
        Expression::CommandCall(_)
        | Expression::ProcCall(_)
        | Expression::Condition(_)
        | Expression::GameVariable(_)
        | Expression::LocalVariable(_)
        | Expression::JoinedString(_)
        | Expression::StringLiteral(_)
        | Expression::CharacterLiteral(_)
        | Expression::CoordLiteral(_) => true,
        Expression::Arithmetic(b) => has_concrete_type(&b.left) || has_concrete_type(&b.right),
        Expression::Parenthesized(w) => has_concrete_type(&w.expression),
        _ => false,
    }
}

/// The literal text as an unbounded integer, then the range rule for
/// the radix (decimal is signed, hex/bin are unsigned and wrap).
fn parse_numeric(value: &str, radix: u32, long: bool) -> Option<NumberValue> {
    let parsed: i128 = if radix == 10 { value.parse().ok()? } else { i128::try_from(u128::from_str_radix(value, radix).ok()?).ok()? };
    match (long, radix == 10) {
        (false, true) => i32::try_from(parsed).ok().map(NumberValue::Int),
        (false, false) => u32::try_from(parsed).ok().map(|v| NumberValue::Int(v as i32)),
        (true, true) => i64::try_from(parsed).ok().map(NumberValue::Long),
        (true, false) => u64::try_from(parsed).ok().map(|v| NumberValue::Long(v as i64)),
    }
}

/// What a dynamic command handler sees.
pub struct TypeCheckingContext<'c, 'a> {
    checker: &'c mut Checker<'a>,
    expression: &'c mut Expression,
}

impl TypeCheckingContext<'_, '_> {
    pub fn types(&mut self) -> &mut TypeManager {
        &mut self.checker.c.types
    }

    /// The first (or with `args2`, second) argument list; an identifier has none.
    fn list(expression: &mut Expression, args2: bool) -> &mut [Expression] {
        match expression {
            Expression::CommandCall(c) => {
                if args2 {
                    c.arguments2.as_deref_mut().unwrap_or_default()
                } else {
                    &mut c.arguments
                }
            }
            _ => &mut [],
        }
    }

    pub fn argument_count(&mut self) -> usize {
        Self::list(self.expression, false).len()
    }

    /// Visits argument `index` (with `hint`) and returns its type; `None` when
    /// there is no such argument or it has no type.
    pub fn check_argument(&mut self, index: usize, hint: Option<TypeId>, args2: bool) -> R<Option<TypeId>> {
        let Some(argument) = Self::list(self.expression, args2).get_mut(index) else { return Ok(None) };
        if hint.is_some() {
            argument.set_type_hint(hint);
        }
        self.checker.expr(argument)?;
        Ok(argument.ty())
    }

    /// The argument must be an identifier naming a type; returns its meta type
    /// (`None` when there is no such argument).
    pub fn check_type_argument(&mut self, index: usize) -> R<Option<TypeId>> {
        let Some(argument) = Self::list(self.expression, false).get_mut(index) else { return Ok(None) };
        let Expression::Identifier(i) = argument else {
            let src = argument.source().clone();
            self.checker.error(&src, "expected a type name", vec![]);
            argument.set_ty(Some(ERROR));
            return Ok(Some(ERROR));
        };
        let Some(t) = self.checker.c.types.find_or_null(&i.text, false) else {
            let text = i.text.clone();
            self.checker.error(&i.source, msg::GENERIC_INVALID_TYPE, vec![text]);
            i.ty = Some(ERROR);
            return Ok(Some(ERROR));
        };
        let meta = self.checker.c.types.meta_type(t);
        i.ty = Some(meta);
        i.reference = Some(self.checker.c.tables.add(Symbol::Basic { name: i.text.clone(), ty: meta, is_protected: false }));
        Ok(Some(meta))
    }

    /// Visits untyped arguments, compares the list with `expected`.
    pub fn check_argument_types(&mut self, expected: TypeId, report: bool, args2: bool) -> R<bool> {
        let mut types = Vec::new();
        for argument in Self::list(self.expression, args2) {
            if argument.ty().is_none() {
                self.checker.expr(argument)?;
            }
            types.push(argument.ty().unwrap_or(ERROR));
        }
        let actual = self.checker.c.types.from_list(&types);
        let src = self.expression.source().clone();
        self.checker.check_type_match(&src, Some(expected), Some(actual), report)
    }

    pub fn set_type(&mut self, t: Option<TypeId>) {
        self.expression.set_ty(t);
    }

    /// Reports an error on the command expression.
    pub fn report_error(&mut self, message: &str, a: Vec<String>) {
        let src = self.expression.source().clone();
        self.checker.error(&src, message, a);
    }

    /// Reports an error on argument `index` of the first argument list.
    pub fn report_argument_error(&mut self, index: usize, message: &str, a: Vec<String>) {
        let src = Self::list(self.expression, false)[index].source().clone();
        self.checker.error(&src, message, a);
    }
}
