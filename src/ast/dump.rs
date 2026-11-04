//! Renders the AST in the golden `ast.json` shape: each node's own fields,
//! `$node` = node type name, `null` kept, unset slots omitted. Field sets are
//! frozen, including the deliberate `readonly` field (in place of `kind`) on
//! `EmptyStatement`.

use serde_json::{Value, json};

use super::expr::*;
use super::stmt::{Statement, SwitchCase};
use super::{Loc, Parameter, Script, ScriptFile, Token};
use crate::symbol::{Symbol, SymbolId, SymbolTables};
use crate::types::{TypeId, TypeManager};

/// What typed fields resolve against: `{"$type": repr}` and symbol representations.
#[derive(Clone, Copy)]
pub struct Dump<'a> {
    pub types: &'a TypeManager,
    pub tables: &'a SymbolTables,
}

impl Dump<'_> {
    pub fn script_file(&self, f: &ScriptFile) -> Value {
        json!({ "$node": "ScriptFile", "source": loc(&f.source), "scripts": list(&f.scripts, |s| self.script(s)) })
    }

    fn ty(&self, t: Option<TypeId>) -> Value {
        t.map_or(Value::Null, |t| json!({ "$type": self.types[t].representation }))
    }

    /// A symbol's representation.
    fn symbol(&self, s: SymbolId) -> Value {
        let rep = |t: TypeId| self.types[t].representation.clone();
        match &self.tables[s] {
            Symbol::LocalVariable { name, ty } => json!({ "$sym": "LocalVariableSymbol", "name": name, "type": rep(*ty) }),
            Symbol::Basic { name, ty, is_protected } => {
                json!({ "$sym": "BasicSymbol", "name": name, "type": rep(*ty), "isProtected": is_protected })
            }
            Symbol::Constant { name, value } => json!({ "$sym": "ConstantSymbol", "name": name, "value": value }),
            Symbol::Script {
                trigger,
                name,
                parameters,
                returns,
            } => json!({
                "$sym": "ServerScriptSymbol",
                "trigger": trigger.get().identifier, "name": name, "parameters": rep(*parameters), "returns": rep(*returns),
            }),
        }
    }

    fn script(&self, s: &Script) -> Value {
        let mut v = json!({
            "$node": "Script", "source": loc(&s.source), "kind": "Script",
            "trigger": self.identifier(&s.trigger), "name": self.identifier(&s.name), "isStar": s.is_star,
            "parameters": opt_list(&s.parameters, |p| self.parameter(p)), "returnTokens": opt_list(&s.return_tokens, token),
            "statements": list(&s.statements, |s| self.statement(s)),
            "subjectReference": s.subject_reference.map_or(Value::Null, |r| self.symbol(r)),
        });
        // Unset slots are omitted.
        let m = v.as_object_mut().unwrap();
        if let Some(t) = s.trigger_type {
            m.insert("triggerType".into(), json!({ "$trigger": t.get().identifier }));
        }
        for (k, t) in [("parameterType", s.parameter_type), ("returnType", s.return_type)] {
            if t.is_some() {
                m.insert(k.into(), self.ty(t));
            }
        }
        if let Some(sym) = s.symbol {
            m.insert("symbol".into(), self.symbol(sym));
        }
        v
    }

    fn parameter(&self, p: &Parameter) -> Value {
        let mut v =
            json!({ "$node": "Parameter", "source": loc(&p.source), "typeToken": token(&p.type_token), "name": self.identifier(&p.name) });
        if let Some(sym) = p.symbol {
            v.as_object_mut().unwrap().insert("symbol".into(), self.symbol(sym));
        }
        v
    }

    /// Common `Node` + `Expression` fields plus `kind`.
    fn expr_node(&self, node: &str, source: &Loc, t: Option<TypeId>, hint: Option<TypeId>) -> serde_json::Map<String, Value> {
        let mut m = serde_json::Map::new();
        m.insert("$node".into(), node.into());
        m.insert("source".into(), loc(source));
        m.insert("type".into(), self.ty(t));
        m.insert("typeHint".into(), self.ty(hint));
        m.insert("kind".into(), node.into());
        m
    }

    fn identifier(&self, i: &Identifier) -> Value {
        with(
            self.expr_node("Identifier", &i.source, i.ty, i.type_hint),
            json!({ "text": i.text, "reference": self.reference(i.reference) }),
        )
    }

    pub fn expression(&self, e: &Expression) -> Value {
        match e {
            Expression::Arithmetic(b) => self.binary("ArithmeticExpression", b),
            Expression::Condition(b) => self.binary("ConditionExpression", b),
            Expression::Calc(w) => self.wrapped("CalcExpression", w),
            Expression::Parenthesized(w) => self.wrapped("ParenthesizedExpression", w),
            Expression::CommandCall(c) => {
                let mut fields = json!({ "name": self.identifier(&c.name), "arguments": list(&c.arguments, |x| self.expression(x)), "arguments2": opt_list(&c.arguments2, |x| self.expression(x)) });
                if let Some(h) = &c.hook {
                    let triggers: Vec<Value> = h.triggers.iter().map(|(n, s)| json!([n, s.is_some()])).collect();
                    fields["hook"] = json!({ "call": self.expression(&h.call), "triggers": triggers });
                }
                with(self.expr_node("CommandCallExpression", &c.source, c.ty, c.type_hint), self.call_symbol(fields, c.symbol))
            }
            Expression::ProcCall(c) => self.call("ProcCallExpression", c),
            Expression::JumpCall(c) => self.call("JumpCallExpression", c),
            Expression::Identifier(i) => self.identifier(i),
            Expression::JoinedString(j) => with(
                self.expr_node("JoinedStringExpression", &j.source, j.ty, j.type_hint),
                json!({ "parts": list(&j.parts, |x| self.string_part(x)) }),
            ),
            Expression::BooleanLiteral(l) => self.literal("BooleanLiteral", l, json!(l.value)),
            Expression::CharacterLiteral(l) => self.literal("CharacterLiteral", l, json!(l.value.to_string())),
            Expression::CoordLiteral(l) => self.literal("CoordLiteral", l, json!(l.value)),
            Expression::NullLiteral(l) => self.literal("NullLiteral", l, json!(l.value)),
            Expression::IntegerLiteral(l) => with(
                self.expr_node("IntegerLiteral", &l.source, l.ty, l.type_hint),
                json!({
                    "value": l.value, "reference": self.reference(l.reference), "radix": l.radix,
                    "numberValue": match l.number_value {
                        None => Value::Null,
                        Some(NumberValue::Int(n)) => json!(n),
                        Some(NumberValue::Long(n)) => json!(format!("{n}n")),
                    },
                }),
            ),
            Expression::StringLiteral(l) => with(
                self.expr_node("StringLiteral", &l.source, l.ty, l.type_hint),
                json!({ "value": l.value, "reference": self.reference(l.reference), "subExpression": l.sub_expression.as_deref().map(|x| self.expression(x)) }),
            ),
            Expression::ConstantVariable(c) => with(
                self.expr_node("ConstantVariableExpression", &c.source, c.ty, c.type_hint),
                json!({ "name": self.identifier(&c.name), "reference": self.reference(c.reference), "subExpression": c.sub_expression.as_deref().map(|x| self.expression(x)) }),
            ),
            Expression::GameVariable(g) => with(
                self.expr_node("GameVariableExpression", &g.source, g.ty, g.type_hint),
                json!({ "name": self.identifier(&g.name), "reference": self.reference(g.reference), "dot": g.dot }),
            ),
            Expression::LocalVariable(v) => with(
                self.expr_node("LocalVariableExpression", &v.source, v.ty, v.type_hint),
                json!({ "name": self.identifier(&v.name), "reference": self.reference(v.reference), "index": v.index.as_deref().map(|x| self.expression(x)) }),
            ),
        }
    }

    fn binary(&self, node: &str, b: &BinaryExpression) -> Value {
        with(
            self.expr_node(node, &b.source, b.ty, b.type_hint),
            json!({ "left": self.expression(&b.left), "operator": token(&b.operator), "right": self.expression(&b.right) }),
        )
    }

    fn wrapped(&self, node: &str, w: &WrappedExpression) -> Value {
        with(
            self.expr_node(node, &w.source, w.ty, w.type_hint),
            json!({ "expression": self.expression(&w.expression) }),
        )
    }

    fn call(&self, node: &str, c: &CallExpression) -> Value {
        with(
            self.expr_node(node, &c.source, c.ty, c.type_hint),
            self.call_symbol(json!({ "name": self.identifier(&c.name), "arguments": list(&c.arguments, |x| self.expression(x)) }), c.symbol),
        )
    }

    /// `null` reference slot or the symbol's representation.
    fn reference(&self, r: Option<SymbolId>) -> Value {
        r.map_or(Value::Null, |s| self.symbol(s))
    }

    /// Adds a call's `symbol` (`null` until resolved).
    fn call_symbol(&self, mut fields: Value, symbol: Option<SymbolId>) -> Value {
        fields.as_object_mut().unwrap().insert("symbol".into(), self.reference(symbol));
        fields
    }

    fn literal<T>(&self, node: &str, l: &Literal<T>, value: Value) -> Value {
        with(
            self.expr_node(node, &l.source, l.ty, l.type_hint),
            json!({ "value": value, "reference": self.reference(l.reference) }),
        )
    }

    fn string_part(&self, p: &StringPart) -> Value {
        match p {
            StringPart::Basic { source, value } => {
                json!({ "$node": "BasicStringPart", "source": loc(source), "kind": "BasicStringPart", "value": value })
            }
            StringPart::PTag { source, value } => {
                json!({ "$node": "PTagStringPart", "source": loc(source), "kind": "PTagStringPart", "value": value })
            }
            StringPart::Expression { source, expression: e } => {
                json!({ "$node": "ExpressionStringPart", "source": loc(source), "kind": "ExpressionStringPart", "expression": self.expression(e) })
            }
        }
    }

    pub fn statement(&self, s: &Statement) -> Value {
        match s {
            Statement::Block { source, statements } => stmt(
                "BlockStatement",
                source,
                json!({ "statements": list(statements, |x| self.statement(x)) }),
            ),
            Statement::Return { source, expressions } => stmt(
                "ReturnStatement",
                source,
                json!({ "expressions": list(expressions, |x| self.expression(x)) }),
            ),
            Statement::If {
                source,
                condition,
                then_statement,
                else_statement,
            } => stmt(
                "IfStatement",
                source,
                json!({ "condition": self.expression(condition), "thenStatement": self.statement(then_statement), "elseStatement": else_statement.as_deref().map(|x| self.statement(x)) }),
            ),
            Statement::While {
                source,
                condition,
                then_statement,
            } => stmt(
                "WhileStatement",
                source,
                json!({ "condition": self.expression(condition), "thenStatement": self.statement(then_statement) }),
            ),
            Statement::Switch { source, type_token, condition, cases, ty, default_case } => {
                let mut v = stmt(
                    "SwitchStatement",
                    source,
                    json!({
                        "typeToken": token(type_token), "condition": self.expression(condition), "cases": list(cases, |x| self.switch_case(x)),
                        "defaultCase": default_case.map_or(Value::Null, |i| self.switch_case(&cases[i])),
                    }),
                );
                v.as_object_mut().unwrap().insert("type".into(), self.ty(*ty));
                v
            }
            Statement::Declaration { source, type_token, name, initializer, symbol } => self.with_symbol(
                stmt(
                    "DeclarationStatement",
                    source,
                    json!({ "typeToken": token(type_token), "name": self.identifier(name), "initializer": initializer.as_ref().map(|x| self.expression(x)) }),
                ),
                *symbol,
            ),
            Statement::ArrayDeclaration { source, type_token, name, initializer, symbol } => self.with_symbol(
                stmt(
                    "ArrayDeclarationStatement",
                    source,
                    json!({ "typeToken": token(type_token), "name": self.identifier(name), "initializer": self.expression(initializer) }),
                ),
                *symbol,
            ),
            Statement::Assignment { source, vars, expressions } => stmt(
                "AssignmentStatement",
                source,
                json!({ "vars": list(vars, |x| self.expression(x)), "expressions": list(expressions, |x| self.expression(x)) }),
            ),
            Statement::Expression { source, expression: e } => {
                stmt("ExpressionStatement", source, json!({ "expression": self.expression(e) }))
            }
            Statement::Empty { source } => json!({ "$node": "EmptyStatement", "source": loc(source), "readonly": "EmptyStatement" }),
        }
    }

    /// Adds a declaration's `symbol` once set (omitted until then).
    fn with_symbol(&self, mut v: Value, symbol: Option<SymbolId>) -> Value {
        if let Some(s) = symbol {
            v.as_object_mut().unwrap().insert("symbol".into(), self.symbol(s));
        }
        v
    }

    fn switch_case(&self, c: &SwitchCase) -> Value {
        stmt(
            "SwitchCase",
            &c.source,
            json!({ "keys": list(&c.keys, |x| self.expression(x)), "statements": list(&c.statements, |x| self.statement(x)) }),
        )
    }
}

fn loc(l: &Loc) -> Value {
    serde_json::to_value(l).unwrap()
}

fn list<T>(items: &[T], f: impl Fn(&T) -> Value) -> Value {
    Value::Array(items.iter().map(f).collect())
}

fn opt_list<T>(items: &Option<Vec<T>>, f: impl Fn(&T) -> Value) -> Value {
    items.as_ref().map_or(Value::Null, |v| list(v, f))
}

fn token(t: &Token) -> Value {
    json!({ "$node": "Token", "source": loc(&t.source), "text": t.text })
}

fn with(mut m: serde_json::Map<String, Value>, fields: Value) -> Value {
    m.extend(fields.as_object().unwrap().clone());
    Value::Object(m)
}

fn stmt(node: &str, source: &Loc, fields: Value) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("$node".into(), node.into());
    m.insert("source".into(), loc(source));
    m.insert("kind".into(), node.into());
    with(m, fields)
}
