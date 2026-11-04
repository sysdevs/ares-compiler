//! One [`RuneScript`] → its binary form (big-endian).
//!
//! Every value is range-checked as it is written: a value that does not fit
//! its field is a [`FatalError`], never truncated. Strings are one byte per
//! char (the low byte of its code point, `?` where that byte is NUL),
//! NUL-terminated.

use std::collections::HashMap;

use ares_shared::opcode::{large_operand, op};
use ares_shared::trigger::{lookup_key, Subject};

use crate::codegen::ir::{Opcode, Operand, RuneScript, SwitchKey};
use crate::runescript::compiler::ServerScriptCompiler;
use crate::runescript::{triggers, types as sv};
use crate::symbol::{Symbol, SymbolId};
use crate::trigger::SubjectMode;
use crate::types::{BaseVarType, TypeKind, COORD, MAPZONE};
use crate::FatalError;

type R<T = ()> = Result<T, FatalError>;

/// `v` as `T`, or a [`FatalError`] when it does not fit.
fn fit<T: TryFrom<i64>>(v: impl Into<i64>) -> R<T> {
    let v = v.into();
    T::try_from(v).map_err(|_| FatalError(format!("Value {v} does not fit in {}.", std::any::type_name::<T>())))
}

fn u16_be(v: impl Into<i64>) -> R<[u8; 2]> {
    Ok(fit::<u16>(v)?.to_be_bytes())
}

fn i32_be(v: impl Into<i64>) -> R<[u8; 4]> {
    Ok(fit::<i32>(v)?.to_be_bytes())
}

fn len(n: usize) -> i64 {
    n as i64
}

/// One byte per char, then NUL. A char whose low byte is NUL (`Ā`, `😀`)
/// becomes `?`, so it cannot end the string early.
fn put_string(buf: &mut Vec<u8>, s: &str) {
    buf.extend(s.chars().map(|c| match c as u32 as u8 {
        0 => b'?',
        b => b,
    }));
    buf.push(0);
}

struct Context<'a> {
    cur_index: usize,
    jump_table: HashMap<&'a str, usize>,
    instructions: Vec<u8>,
    instruction_count: i32,
    switches: Vec<u8>,
}

impl Context<'_> {
    /// A 4-byte operand for `large_operand` opcodes, else 1 byte.
    fn instruction(&mut self, op: i32, operand: impl Into<i64>) -> R {
        self.instruction_count += 1;
        self.instructions.extend(u16_be(op)?);
        if large_operand(op) {
            self.instructions.extend(i32_be(operand)?);
        } else {
            self.instructions.push(fit(operand)?);
        }
        Ok(())
    }

    fn instruction_raw(&mut self, op: i32, operand: u8) -> R {
        self.instruction_count += 1;
        self.instructions.extend(u16_be(op)?);
        self.instructions.push(operand);
        Ok(())
    }

    fn instruction_string(&mut self, op: i32, s: &str) -> R {
        self.instruction_count += 1;
        self.instructions.extend(u16_be(op)?);
        put_string(&mut self.instructions, s);
        Ok(())
    }

    fn jump(&self, label: &str) -> R<i64> {
        let at = *self.jump_table.get(label).ok_or_else(|| FatalError(format!("Label not found: {label}.")))?;
        Ok(len(at) - len(self.cur_index) - 1)
    }
}

/// The script's binary form.
pub fn write_script(c: &ServerScriptCompiler, script: &RuneScript) -> R<Vec<u8>> {
    let w = Writer { c, script };
    let lookup_key = w.lookup_key()?;
    // Line number table (sourced instructions, on change) and jump table.
    let mut line_table: Vec<(usize, i32)> = Vec::new();
    let mut prev_line = -1;
    let mut jump_table = HashMap::new();
    let mut index = 0;
    for block in &script.blocks {
        jump_table.insert(block.label.name.as_str(), index);
        for inst in &block.instructions {
            if let Some(line) = inst.source.as_ref().map(|s| s.line)
                && line != prev_line
            {
                line_table.push((index, line));
                prev_line = line;
            }
            index += 1;
        }
    }
    let mut ctx = Context { cur_index: 0, jump_table, instructions: Vec::new(), instruction_count: 0, switches: Vec::new() };
    for block in &script.blocks {
        for inst in &block.instructions {
            w.instruction(&mut ctx, inst.opcode, &inst.operand)?;
            ctx.cur_index += 1;
        }
    }
    w.finish(&ctx, lookup_key, &line_table)
}

struct Writer<'a> {
    c: &'a ServerScriptCompiler,
    script: &'a RuneScript,
}

impl Writer<'_> {
    fn id(&self, s: SymbolId) -> R<i32> {
        self.c.mapper.get(&self.c.tables, s)
    }

    fn ty_of(&self, s: SymbolId) -> crate::types::TypeId {
        match self.c.tables[s] {
            Symbol::LocalVariable { ty, .. } | Symbol::Basic { ty, .. } => ty,
            _ => unreachable!("typed symbol"),
        }
    }

    /// The trigger lookup key ([`lookup_key`]); `-1` for name-mode triggers.
    fn lookup_key(&self) -> R<i32> {
        let trigger = self.script.trigger.get();
        if trigger.subject_mode == SubjectMode::Name {
            return Ok(-1);
        }
        let mut subject = Subject::Global;
        if let (SubjectMode::Type { .. }, Some(s)) = (trigger.subject_mode, self.script.subject_reference) {
            let ty = self.ty_of(s);
            // Mapzone and coord subjects are named by their packed value.
            let id = if ty == MAPZONE || ty == COORD {
                let name = self.c.tables[s].name();
                name.parse::<i32>().map_err(|_| FatalError(format!("Invalid packed subject: {name}.")))?
            } else {
                self.id(s)?
            };
            subject = if ty == sv::CATEGORY { Subject::Category(id) } else { Subject::Type(id) };
        }
        Ok(lookup_key(trigger.id, subject))
    }

    /// A local's index among the locals of its kind, or `-1`.
    fn variable_id(&self, local: SymbolId) -> i32 {
        let locals = &self.script.locals;
        let types = &self.c.types;
        let is_array = |s: SymbolId| matches!(types[self.ty_of(s)].kind, TypeKind::Array { .. });
        let base = |s: SymbolId| types[self.ty_of(s)].base_type;
        let position = if is_array(local) {
            locals.iter().filter(|&&v| is_array(v)).position(|&v| v == local)
        } else {
            locals
                .iter()
                .filter(|&&v| base(v) == base(local) && (!is_array(v) || self.script.parameters.contains(&v)))
                .position(|&v| v == local)
        };
        position.map_or(-1, |p| p as i32)
    }

    fn instruction(&self, ctx: &mut Context, opcode: Opcode, operand: &Operand) -> R {
        let types = &self.c.types;
        let symbol = || match operand {
            Operand::Symbol(s) => *s,
            _ => unreachable!("symbol operand"),
        };
        match opcode {
            Opcode::PushConstantInt => {
                let Operand::Int(n) = operand else { unreachable!() };
                ctx.instruction(op::PUSH_CONSTANT_INT, *n)
            }
            Opcode::PushConstantString => {
                let Operand::String(s) = operand else { unreachable!() };
                ctx.instruction_string(op::PUSH_CONSTANT_STRING, s)
            }
            Opcode::PushConstantLong => Err(FatalError("Not supported.".into())),
            Opcode::PushConstantSymbol => {
                let s = symbol();
                let id = match &self.c.tables[s] {
                    Symbol::LocalVariable { .. } => i64::from(self.variable_id(s)),
                    Symbol::Basic { ty, .. } if matches!(types[*ty].kind, TypeKind::MetaType { .. }) => {
                        let TypeKind::MetaType { inner } = types[*ty].kind else { unreachable!() };
                        match types[inner].code()? {
                            Some(c) => i64::from(c),
                            None => return Err(FatalError(format!("Type has no code: {}.", types[inner].representation))),
                        }
                    }
                    _ => i64::from(self.id(s)?),
                };
                ctx.instruction(op::PUSH_CONSTANT_INT, id)
            }
            Opcode::PushLocalVar | Opcode::PopLocalVar => {
                let s = symbol();
                let id = self.variable_id(s);
                let ty = &types[self.ty_of(s)];
                let push = opcode == Opcode::PushLocalVar;
                let op = match (&ty.kind, ty.base_type, push) {
                    (TypeKind::Array { .. }, _, true) => op::PUSH_ARRAY_INT,
                    (TypeKind::Array { .. }, _, false) => op::POP_ARRAY_INT,
                    (_, Some(BaseVarType::String), true) => op::PUSH_STRING_LOCAL,
                    (_, Some(BaseVarType::String), false) => op::POP_STRING_LOCAL,
                    (_, Some(BaseVarType::Integer), true) => op::PUSH_INT_LOCAL,
                    (_, Some(BaseVarType::Integer), false) => op::POP_INT_LOCAL,
                    _ => return Err(FatalError(format!("Unsupported local variable type: {}.", ty.representation))),
                };
                ctx.instruction(op, id)
            }
            Opcode::PushVar | Opcode::PushVar2 | Opcode::PopVar | Opcode::PopVar2 => {
                let s = symbol();
                let id = self.id(s)?;
                let push = matches!(opcode, Opcode::PushVar | Opcode::PushVar2);
                let op = match (&types[self.ty_of(s)].kind, push) {
                    (TypeKind::VarPlayer { .. }, true) => op::PUSH_VARP,
                    (TypeKind::VarBit { .. }, true) => op::PUSH_VARBIT,
                    (TypeKind::VarNpc { .. }, true) => op::PUSH_VARN,
                    (TypeKind::VarObj { .. }, true) => op::PUSH_VARO,
                    (TypeKind::VarShared { .. }, true) => op::PUSH_VARS,
                    (TypeKind::VarPlayer { .. }, false) => op::POP_VARP,
                    (TypeKind::VarBit { .. }, false) => op::POP_VARBIT,
                    (TypeKind::VarNpc { .. }, false) => op::POP_VARN,
                    (TypeKind::VarObj { .. }, false) => op::POP_VARO,
                    (TypeKind::VarShared { .. }, false) => op::POP_VARS,
                    // A varc reads and writes by its value's stack.
                    (TypeKind::VarClient { inner }, push) => match (types[*inner].base_type, push) {
                        (Some(BaseVarType::String), true) => op::GET_VARC_STRING,
                        (Some(BaseVarType::String), false) => op::SET_VARC_STRING,
                        (_, true) => op::GET_VARC_INT,
                        (_, false) => op::SET_VARC_INT,
                    },
                    _ => return Err(FatalError("Unsupported variable type.".into())),
                };
                let dot = matches!(opcode, Opcode::PushVar2 | Opcode::PopVar2);
                ctx.instruction(op, i64::from(id) + if dot { 65536 } else { 0 })
            }
            Opcode::DefineArray => {
                let s = symbol();
                let id = self.variable_id(s);
                let TypeKind::Array { inner } = types[self.ty_of(s)].kind else { unreachable!("array local") };
                let Some(code) = types[inner].code()? else {
                    return Err(FatalError(format!("Type has no code: {}.", types[inner].representation)));
                };
                ctx.instruction(op::DEFINE_ARRAY, id.wrapping_shl(16) | code as i32)
            }
            Opcode::Switch => {
                let Operand::Switch(id) = operand else { unreachable!() };
                ctx.instruction(op::SWITCH, len(*id))?;
                let size_pos = ctx.switches.len();
                ctx.switches.extend([0, 0]);
                let mut total = 0usize;
                for case in &self.script.switch_tables[*id].cases {
                    let jump = ctx.jump(&case.label.name)?;
                    for key in &case.keys {
                        let value = match key {
                            SwitchKey::Int(n) => *n,
                            SwitchKey::Bool(b) => *b as i32,
                            SwitchKey::Symbol(s) => self.id(*s)?,
                            SwitchKey::String(_) | SwitchKey::Long(_) => return Err(FatalError("Unable to find id for switch key.".into())),
                        };
                        ctx.switches.extend(value.to_be_bytes());
                        ctx.switches.extend(i32_be(jump)?);
                    }
                    total += case.keys.len();
                }
                let size = u16_be(len(total))?;
                ctx.switches[size_pos..size_pos + 2].copy_from_slice(&size);
                Ok(())
            }
            Opcode::Branch
            | Opcode::BranchNot
            | Opcode::BranchEquals
            | Opcode::BranchLessThan
            | Opcode::BranchGreaterThan
            | Opcode::BranchLessThanOrEquals
            | Opcode::BranchGreaterThanOrEquals => {
                let op = match opcode {
                    Opcode::Branch => op::BRANCH,
                    Opcode::BranchNot => op::BRANCH_NOT,
                    Opcode::BranchEquals => op::BRANCH_EQUALS,
                    Opcode::BranchLessThan => op::BRANCH_LESS_THAN,
                    Opcode::BranchGreaterThan => op::BRANCH_GREATER_THAN,
                    Opcode::BranchLessThanOrEquals => op::BRANCH_LESS_THAN_OR_EQUALS,
                    _ => op::BRANCH_GREATER_THAN_OR_EQUALS,
                };
                let Operand::Label(l) = operand else { unreachable!() };
                let jump = ctx.jump(&l.name)?;
                ctx.instruction(op, jump)
            }
            Opcode::JoinString => {
                let Operand::Int(n) = operand else { unreachable!() };
                ctx.instruction(op::JOIN_STRING, *n)
            }
            Opcode::Discard => match operand {
                Operand::BaseVar(BaseVarType::Integer) => ctx.instruction(op::POP_INT_DISCARD, 0),
                Operand::BaseVar(BaseVarType::String) => ctx.instruction(op::POP_STRING_DISCARD, 0),
                other => Err(FatalError(format!("Unsupported discard type: {other:?}."))),
            },
            Opcode::Gosub => ctx.instruction(op::GOSUB_WITH_PARAMS, self.id(symbol())?),
            Opcode::Jump => ctx.instruction(op::JUMP_WITH_PARAMS, self.id(symbol())?),
            Opcode::Command => {
                let s = symbol();
                let op = self.id(s)?;
                if op == -1 {
                    return Err(FatalError(format!("Missing opcode id for command {}", self.c.tables[s].name())));
                }
                // A command is written with a one-byte operand, so its id must
                // not be an opcode the server reads a 4-byte operand for.
                if large_operand(op) {
                    return Err(FatalError(format!("Command {} has id {op}, an opcode with a 4-byte operand.", self.c.tables[s].name())));
                }
                let secondary = self.c.tables[s].name().starts_with('.');
                ctx.instruction_raw(op, secondary as u8)
            }
            Opcode::Return => ctx.instruction(op::RETURN, 0),
            Opcode::Add => ctx.instruction(op::ADD, 0),
            Opcode::Sub => ctx.instruction(op::SUB, 0),
            Opcode::Multiply => ctx.instruction(op::MULTIPLY, 0),
            Opcode::Divide => ctx.instruction(op::DIVIDE, 0),
            Opcode::Modulo => ctx.instruction(op::MODULO, 0),
            Opcode::Or => ctx.instruction(op::OR, 0),
            Opcode::And => ctx.instruction(op::AND, 0),
            other => Err(FatalError(format!("Unsupported opcode: {other:?}."))),
        }
    }

    /// Header, line table, instructions, local and parameter counts, switch tables.
    fn finish(&self, ctx: &Context, lookup_key: i32, line_table: &[(usize, i32)]) -> R<Vec<u8>> {
        let types = &self.c.types;
        let mut buf = Vec::new();
        put_string(&mut buf, &self.script.full_name);
        put_string(&mut buf, &self.script.source_name);
        buf.extend(lookup_key.to_be_bytes());
        if self.script.trigger == triggers::DEBUGPROC {
            let Symbol::Script { parameters, .. } = self.c.tables[self.script.symbol] else { unreachable!("script symbol") };
            let params = types.to_list(Some(parameters));
            buf.push(fit(len(params.len()))?);
            for p in params {
                // The parameter's type code, or 0xff (-1) for none.
                buf.push(types[p].code()?.unwrap_or(0xff));
            }
        } else {
            buf.push(0);
        }
        buf.extend(u16_be(len(line_table.len()))?);
        for &(pc, line) in line_table {
            buf.extend(i32_be(len(pc))?);
            buf.extend(line.to_be_bytes());
        }
        buf.extend(&ctx.instructions);
        buf.extend(ctx.instruction_count.to_be_bytes());
        for count in [self.local_count(BaseVarType::Integer), self.local_count(BaseVarType::String)] {
            buf.extend(u16_be(len(count))?);
        }
        for base in [BaseVarType::Integer, BaseVarType::String] {
            let count = self.script.parameters.iter().filter(|&&p| types[self.ty_of(p)].base_type == Some(base)).count();
            buf.extend(u16_be(len(count))?);
        }
        buf.push(fit(len(self.script.switch_tables.len()))?);
        buf.extend(&ctx.switches);
        buf.extend(u16_be(len(ctx.switches.len() + 1))?);
        Ok(buf)
    }

    /// Locals of `base`; arrays only count when they are parameters.
    fn local_count(&self, base: BaseVarType) -> usize {
        let types = &self.c.types;
        self.script
            .locals
            .iter()
            .filter(|&&v| {
                let t = &types[self.ty_of(v)];
                t.base_type == Some(base) && (!matches!(t.kind, TypeKind::Array { .. }) || self.script.parameters.contains(&v))
            })
            .count()
    }
}
