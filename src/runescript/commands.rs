//! The dynamic command handlers: the set is closed, so it is one enum with
//! `type_check` and `generate_code`, plus the expression printer used by `dump`.

use crate::ast::expr::Expression;
use crate::codegen::ir::{Opcode, Operand};
use crate::codegen::CodeGeneratorContext;
use crate::semantics::typecheck::TypeCheckingContext;
use crate::symbol::{SymbolTables, SymbolType};
use crate::trigger::COMMAND;
use crate::types::{BaseVarType, TypeId, TypeKind, ANY, ERROR, INT, STRING, UNIT};
use crate::FatalError;

#[derive(Clone, Copy, Debug)]
pub enum CommandHandler {
    Queue(TypeId),
    QueueVarArg(TypeId),
    LongQueue(TypeId),
    LongQueueVarArg(TypeId),
    Timer(TypeId),
    /// `param_any` is the one `param` of `any` shared by every param handler.
    Param { ty: Option<TypeId>, param_any: TypeId },
    Enum,
    DbFind { with_count: bool },
    DbGetField,
    Dump,
    Script,
}

type R<T = ()> = Result<T, FatalError>;

impl CommandHandler {
    pub fn type_check(self, ctx: &mut TypeCheckingContext) -> R {
        match self {
            CommandHandler::Queue(q) => fixed(ctx, &[q, INT, INT]),
            CommandHandler::LongQueue(q) => fixed(ctx, &[q, INT, INT, INT]),
            CommandHandler::QueueVarArg(q) => var_arg(ctx, q, &[q, INT]),
            CommandHandler::LongQueueVarArg(q) => var_arg(ctx, q, &[q, INT, INT]),
            CommandHandler::Timer(t) => {
                let timer = ctx.check_argument(0, Some(t), false)?;
                ctx.check_argument(1, Some(INT), false)?;
                let mut expected = vec![t, INT];
                expected.extend(script_params(ctx, timer, t));
                let expected = ctx.types().from_list(&expected);
                ctx.check_argument_types(expected, true, false)?;
                ctx.set_type(Some(UNIT));
                Ok(())
            }
            CommandHandler::Param { ty, param_any } => {
                let mut expected = Vec::new();
                if let Some(t) = ty {
                    expected.push(t);
                    ctx.check_argument(0, Some(t), false)?;
                }
                let index = if ty.is_none() { 0 } else { 1 };
                let param = ctx.check_argument(index, Some(param_any), false)?;
                let ret = param.and_then(|p| match ctx.types()[p].kind {
                    TypeKind::Param { inner } => Some(inner),
                    _ => None,
                });
                let p = ctx.types().param(ret.unwrap_or(ANY));
                expected.push(p);
                let expected = ctx.types().from_list(&expected);
                if !ctx.check_argument_types(expected, true, false)? {
                    ctx.set_type(Some(ERROR));
                    return Ok(());
                }
                match ret {
                    None => {
                        ctx.report_error("cannot find the param's return type", vec![]);
                        ctx.set_type(Some(ERROR));
                    }
                    Some(r) => ctx.set_type(Some(r)),
                }
                Ok(())
            }
            CommandHandler::Enum => {
                let input = ctx.check_type_argument(0)?;
                let output = ctx.check_type_argument(1)?;
                ctx.check_argument(2, Some(crate::runescript::types::ENUM), false)?;
                let inner = |ctx: &mut TypeCheckingContext, t: Option<TypeId>| {
                    t.and_then(|t| match ctx.types()[t].kind {
                        TypeKind::MetaType { inner } => Some(inner),
                        _ => None,
                    })
                };
                let (input, output) = (inner(ctx, input), inner(ctx, output));
                ctx.check_argument(3, input, false)?;
                let types = ctx.types();
                let (a, b) = (types.meta_type(input.unwrap_or(ANY)), types.meta_type(output.unwrap_or(ANY)));
                let expected = types.tuple(&[a, b, crate::runescript::types::ENUM, input.unwrap_or(ANY)])?;
                ctx.check_argument_types(expected, true, false)?;
                ctx.set_type(Some(output.unwrap_or(ERROR)));
                Ok(())
            }
            CommandHandler::DbFind { with_count } => {
                let any_column = ctx.types().db_column(ANY);
                let column = ctx.check_argument(0, Some(any_column), false)?;
                // Any wrapped type's inner.
                let key = column.and_then(|c| ctx.types()[c].kind.inner());
                ctx.check_argument(1, key, false)?;
                let types = ctx.types();
                let key_column = types.db_column(key.unwrap_or(ANY));
                let expected = types.tuple(&[key_column, key.unwrap_or(ANY)])?;
                if key.is_some_and(|k| matches!(ctx.types()[k].kind, TypeKind::Tuple(_))) {
                    ctx.report_argument_error(0, "tuple columns are not supported", vec![]);
                } else {
                    ctx.check_argument_types(expected, true, false)?;
                }
                ctx.set_type(Some(if with_count { INT } else { UNIT }));
                Ok(())
            }
            CommandHandler::DbGetField => {
                use crate::runescript::types::DBROW;
                ctx.check_argument(0, Some(DBROW), false)?;
                let any_column = ctx.types().db_column(ANY);
                let column = ctx.check_argument(1, Some(any_column), false)?;
                ctx.check_argument(2, Some(INT), false)?;
                let ret = column.and_then(|c| ctx.types()[c].kind.inner());
                let types = ctx.types();
                let ret_column = types.db_column(ret.unwrap_or(ANY));
                let expected = types.tuple(&[DBROW, ret_column, INT])?;
                if !ctx.check_argument_types(expected, true, false)? {
                    ctx.set_type(Some(ERROR));
                    return Ok(());
                }
                if ctx.argument_count() < 2 {
                    ctx.report_error("cannot infer a type from this argument", vec![]);
                    ctx.set_type(Some(ERROR));
                    return Ok(());
                }
                // May be unset; the type checker then reports CUSTOM_HANDLER_NOTYPE.
                ctx.set_type(ret);
                Ok(())
            }
            CommandHandler::Dump => {
                let count = ctx.argument_count();
                if count == 0 {
                    ctx.report_error("mismatched types: expected `any...`, found `<unit>`", vec![]);
                }
                for i in 0..count {
                    let Some(t) = ctx.check_argument(i, Some(ANY), false)? else {
                        return Err(FatalError(format!("Internal compiler error: dump argument {i} has no type.")));
                    };
                    let data = &ctx.types()[t];
                    let rep = data.representation.clone();
                    if let TypeKind::Tuple(_) = data.kind {
                        ctx.report_argument_error(i, "cannot dump a multi-value expression of type `%s`", vec![rep]);
                    } else if !matches!(data.base_type, Some(BaseVarType::Integer | BaseVarType::String)) {
                        ctx.report_argument_error(i, "cannot dump an expression with no value", vec![rep]);
                    }
                }
                ctx.set_type(Some(STRING));
                Ok(())
            }
            CommandHandler::Script => {
                ctx.check_argument_types(UNIT, true, false)?;
                ctx.set_type(Some(STRING));
                Ok(())
            }
        }
    }
}

/// Queue/longqueue: hint each argument with `expected`, check the list, return `unit`.
fn fixed(ctx: &mut TypeCheckingContext, expected: &[TypeId]) -> R {
    for (i, &t) in expected.iter().enumerate() {
        ctx.check_argument(i, Some(t), false)?;
    }
    let expected = ctx.types().from_list(expected);
    ctx.check_argument_types(expected, true, false)?;
    ctx.set_type(Some(UNIT));
    Ok(())
}

/// The `*` queue forms: `base` in the first argument list, the queued script's
/// parameters in the second.
fn var_arg(ctx: &mut TypeCheckingContext, queue: TypeId, base: &[TypeId]) -> R {
    let script = ctx.check_argument(0, Some(queue), false)?;
    for (i, &t) in base.iter().enumerate().skip(1) {
        ctx.check_argument(i, Some(t), false)?;
    }
    let var_args = script_params(ctx, script, queue);
    let (base, var_args) = (ctx.types().from_list(base), ctx.types().from_list(&var_args));
    ctx.check_argument_types(base, true, false)?;
    ctx.check_argument_types(var_args, true, true)?;
    ctx.set_type(Some(UNIT));
    Ok(())
}

/// The parameter list of a script-typed argument whose trigger matches
/// `expected`'s and whose parameter type is not `unit` (identity).
fn script_params(ctx: &mut TypeCheckingContext, actual: Option<TypeId>, expected: TypeId) -> Vec<TypeId> {
    let types = ctx.types();
    let TypeKind::Script { trigger: want, .. } = types[expected].kind else { return Vec::new() };
    match actual.map(|a| &types[a].kind) {
        Some(TypeKind::Script { trigger, parameter_type, .. }) if *trigger == want && *parameter_type != UNIT => {
            types.to_list(Some(*parameter_type))
        }
        _ => Vec::new(),
    }
}

impl CommandHandler {
    /// Handlers without custom code emit their arguments then the command.
    pub fn generate_code(self, ctx: &mut CodeGeneratorContext) -> R {
        match self {
            CommandHandler::QueueVarArg(_) | CommandHandler::LongQueueVarArg(_) => {
                ctx.visit_all(ctx.arguments())?;
                let args2 = ctx.arguments2();
                ctx.visit_all(args2)?;
                let codes = type_codes(ctx, args2)?;
                ctx.instruction(Opcode::PushConstantString, Operand::String(codes));
                ctx.command()
            }
            CommandHandler::Timer(_) => {
                let args = ctx.arguments();
                ctx.visit_all(args)?;
                let codes = if args.len() > 2 { type_codes(ctx, &args[2..])? } else { String::new() };
                ctx.instruction(Opcode::PushConstantString, Operand::String(codes));
                ctx.command()
            }
            CommandHandler::DbFind { .. } => {
                let args = ctx.arguments();
                let Some(stack) = ctx.inner_base_type(args[0].ty()) else {
                    return Err(FatalError("No base type for db_find column".into()));
                };
                ctx.visit_all(args)?;
                ctx.instruction(Opcode::PushConstantInt, Operand::Int(stack as i32));
                ctx.command()
            }
            CommandHandler::Dump => {
                let args = ctx.arguments();
                let compiler = ctx.compiler();
                let mut parts = 0;
                for (i, arg) in args.iter().enumerate() {
                    let text = expression_string(arg)?;
                    ctx.instruction(Opcode::PushConstantString, Operand::String(format!("{text}=")));
                    parts += 1;
                    ctx.visit(arg)?;
                    // The conversion commands are looked up as `escape` and
                    // `toString` (camel case), so usually only `escape` exists.
                    let Some(t) = arg.ty() else {
                        return Err(FatalError(format!("Internal compiler error: dump argument {i} has no type.")));
                    };
                    let name = if t == STRING {
                        "escape"
                    } else if compiler.types[t].base_type == Some(BaseVarType::Integer) {
                        "toString"
                    } else {
                        return Err(FatalError(format!("Unsupported type conversion to string: {}", ctx.type_rep(t))));
                    };
                    if let Some(s) = compiler.tables.find(SymbolTables::ROOT, SymbolType::ServerScript(COMMAND), name) {
                        ctx.instruction(Opcode::Command, Operand::Symbol(s));
                    }
                    if i != args.len() - 1 {
                        ctx.instruction(Opcode::PushConstantString, Operand::String(", ".into()));
                        parts += 1;
                    }
                }
                ctx.instruction(Opcode::JoinString, Operand::Int(parts));
                Ok(())
            }
            CommandHandler::Script => {
                // `[trigger, name]`, with the space (deliberate output).
                let (trigger, name) = ctx.script_header().clone();
                ctx.instruction(Opcode::PushConstantString, Operand::String(format!("[{trigger}, {name}]")));
                Ok(())
            }
            CommandHandler::Queue(_) | CommandHandler::LongQueue(_) | CommandHandler::Param { .. } | CommandHandler::Enum | CommandHandler::DbGetField => {
                ctx.visit_all(ctx.arguments())?;
                ctx.command()
            }
        }
    }
}

/// The joined type codes of `args` (absent codes skipped), one char per code byte.
fn type_codes(ctx: &CodeGeneratorContext, args: &[Expression]) -> R<String> {
    let mut out = String::new();
    for a in args {
        out.extend(ctx.type_code(a.ty())?.map(char::from));
    }
    Ok(out)
}

/// The text `dump` prints for an expression. Deliberate output: every nested
/// node prints as `undefined` (and as nothing in a command's argument list),
/// so `dump($x)` gives `$undefined=`. Nested nodes are still traversed, and a
/// nested kind with no text form is a fatal error.
fn expression_string(e: &Expression) -> R<String> {
    const U: &str = "undefined";
    let not_implemented = |name: &str| Err(FatalError(format!("Not implemented: {name}")));
    // Traverse a child for its errors, discard the text.
    let visit = |child: &Expression| expression_string(child).map(|_| ());
    Ok(match e {
        Expression::Arithmetic(b) | Expression::Condition(b) => {
            visit(&b.left)?;
            visit(&b.right)?;
            format!("{U} {} {U}", b.operator.text)
        }
        Expression::Calc(w) => {
            visit(&w.expression)?;
            format!("calc{U}")
        }
        // Nested arguments print as nothing.
        Expression::CommandCall(c) => {
            c.arguments.iter().try_for_each(visit)?;
            format!("~{U}{}", if c.arguments.is_empty() { "" } else { "()" })
        }
        Expression::LocalVariable(_) => format!("${U}"),
        Expression::GameVariable(_) => format!("%{U}"),
        Expression::ConstantVariable(_) => format!("^{U}"),
        Expression::IntegerLiteral(l) => match l.radix {
            16 => format!("0x{}", l.value),
            2 => format!("0b{}", l.value),
            _ => l.value.clone(),
        },
        Expression::CharacterLiteral(l) => format!("'{}'", l.value),
        Expression::NullLiteral(_) => "null".into(),
        Expression::StringLiteral(l) => format!("\"{}\"", l.value),
        Expression::BooleanLiteral(l) => l.value.to_string(),
        Expression::CoordLiteral(l) => l.value.to_string(),
        Expression::JoinedString(j) => {
            for part in &j.parts {
                if let crate::ast::expr::StringPart::Expression { expression, .. } = part {
                    visit(expression)?;
                }
            }
            format!("\"{}\"", U.repeat(j.parts.len()))
        }
        Expression::Identifier(i) => i.text.clone(),
        Expression::Parenthesized(_) => return not_implemented("ParenthesizedExpression"),
        Expression::ProcCall(_) => return not_implemented("ProcCallExpression"),
        Expression::JumpCall(_) => return not_implemented("JumpCallExpression"),
    })
}
