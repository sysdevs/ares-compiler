//! Script registration: resolves each script's trigger, subject, parameter
//! and return types, and inserts its symbol into the root table. Runs over
//! every file before type checking. A redeclared script fails to insert and is
//! reported, so the first declaration in walk order wins.

use crate::ast::{Loc, Parameter, Script, ScriptFile};
use crate::diagnostics::messages as msg;
use crate::diagnostics::{args, DiagnosticType, Diagnostics};
use crate::features::StrictFeatureLevel;
use crate::runescript::compiler::ServerScriptCompiler;
use crate::runescript::{triggers, types as sv};
use crate::symbol::{ScopeId, Symbol, SymbolId, SymbolTables, SymbolType};
use crate::trigger::{SubjectMode, TriggerId, COMMAND};
use crate::types::{TypeId, COORD, ERROR, MAPZONE, NOTHING, UNIT};

/// Registers every script of `files`, in order.
pub fn register_scripts(c: &mut ServerScriptCompiler, files: &mut [ScriptFile], diagnostics: &mut Diagnostics) {
    // One table for all files, then one per script.
    let base = c.tables.sub_table(SymbolTables::ROOT);
    let mut r = Registration { c, diagnostics };
    for file in files {
        for script in &mut file.scripts {
            let scope = r.c.tables.sub_table(base);
            r.script(script, scope, None);
        }
    }
}

/// [`register_scripts`] for one re-parsed file with the same script headers as
/// `previous`: each script takes the symbol (or the redeclaration error) its
/// counterpart got, instead of declaring itself again.
pub fn reregister_file(c: &mut ServerScriptCompiler, file: &mut ScriptFile, previous: &ScriptFile, diagnostics: &mut Diagnostics) {
    let base = c.tables.sub_table(SymbolTables::ROOT);
    let mut r = Registration { c, diagnostics };
    for (script, old) in file.scripts.iter_mut().zip(&previous.scripts) {
        let scope = r.c.tables.sub_table(base);
        r.script(script, scope, Some(old.symbol));
    }
}

struct Registration<'a> {
    c: &'a mut ServerScriptCompiler,
    diagnostics: &'a mut Diagnostics,
}

impl Registration<'_> {
    fn features(&self) -> StrictFeatureLevel {
        self.c.features
    }

    fn is_disabled_type_name(&self, text: &str) -> bool {
        self.features().is_disabled_type_name(text)
    }

    fn is_disabled_trigger(&self, t: TriggerId) -> bool {
        !self.features().procs && t == triggers::PROC
    }

    /// `reuse`: the symbol this script already has (`Some(None)`: it was a redeclaration).
    fn script(&mut self, script: &mut Script, scope: ScopeId, reuse: Option<Option<SymbolId>>) {
        let trigger = crate::trigger::by_name(&script.trigger.text);
        match trigger {
            None => self.diagnostics.error(&script.trigger.source, msg::SCRIPT_TRIGGER_INVALID, args(&[&script.trigger.text])),
            Some(t) => {
                script.trigger_type = Some(t);
                if self.is_disabled_trigger(t) {
                    self.diagnostics.error(&script.trigger.source, msg::FEATURE_DISABLED_TRIGGER, args(&[t.get().identifier]));
                }
            }
        }
        if script.is_star && trigger != Some(COMMAND) {
            self.diagnostics.error(&script.name.source, msg::SCRIPT_COMMAND_ONLY, vec![]);
        }
        if let Some(t) = trigger {
            self.check_subject(t, script);
        }

        let trigger_type = script.trigger_type;
        let mut parameter_types = Vec::new();
        for p in script.parameters.iter_mut().flatten() {
            parameter_types.push(self.parameter(p, trigger_type, scope));
        }
        script.parameter_type = Some(self.c.types.from_list(&parameter_types));
        if let Some(t) = trigger {
            let params = script.parameters.as_deref().unwrap_or_default();
            if !t.get().allow_parameters && !params.is_empty() {
                self.diagnostics.error(&params[0].source, msg::SCRIPT_TRIGGER_NO_PARAMETERS, args(&[t.get().identifier]));
            }
        }

        script.return_type = Some(match &script.return_tokens {
            Some(tokens) if !tokens.is_empty() => {
                let mut returns = Vec::new();
                for token in tokens {
                    let ty = if self.is_disabled_type_name(&token.text) {
                        self.diagnostics.error(&token.source, msg::FEATURE_DISABLED_TYPE, args(&[&token.text]));
                        Some(ERROR)
                    } else {
                        let ty = self.c.types.find_or_null(&token.text, false);
                        if ty.is_none() {
                            self.diagnostics.error(&token.source, msg::GENERIC_INVALID_TYPE, args(&[&token.text]));
                        }
                        ty
                    };
                    returns.push(ty.unwrap_or(ERROR));
                }
                self.c.types.from_list(&returns)
            }
            _ => match trigger {
                None => ERROR,
                Some(t) if t.get().allow_returns => UNIT,
                Some(_) => NOTHING,
            },
        });
        if let Some(t) = trigger
            && !t.get().allow_returns
            && script.return_type != Some(NOTHING)
        {
            self.diagnostics.error(&script.source, msg::SCRIPT_TRIGGER_NO_RETURNS, args(&[t.get().identifier]));
        }

        if let Some(t) = trigger
            && !self.is_disabled_trigger(t)
        {
            let name = if script.is_star { format!("{}*", script.name.text) } else { script.name.text.clone() };
            if let Some(reuse) = reuse {
                match reuse {
                    Some(symbol) => script.symbol = Some(symbol),
                    None => self.redeclared(script, t, &name),
                }
                script.block = Some(scope);
                return;
            }
            let symbol = self.c.tables.add(Symbol::Script {
                trigger: t,
                name: name.clone(),
                parameters: script.parameter_type.unwrap(),
                returns: script.return_type.unwrap(),
            });
            if self.c.tables.insert(SymbolTables::ROOT, SymbolType::ServerScript(t), symbol) {
                self.c.tables.declared_at(symbol, &header(script));
                script.symbol = Some(symbol);
            } else {
                self.redeclared(script, t, &name);
            }
        }
        script.block = Some(scope);
    }

    fn redeclared(&mut self, script: &Script, t: TriggerId, name: &str) {
        let first = self.c.tables.find(SymbolTables::ROOT, SymbolType::ServerScript(t), name).and_then(|s| self.c.tables.decl(s)).cloned();
        let d = self.diagnostics.emit(DiagnosticType::Error, &header(script), msg::SCRIPT_REDECLARATION, args(&[t.get().identifier, name]));
        d.label("redefined here");
        d.note(first.as_ref(), "first defined here");
    }

    /// The subject check and the global/category/type checks it dispatches to.
    fn check_subject(&mut self, trigger: TriggerId, script: &mut Script) {
        let identifier = trigger.get().identifier;
        let mode = trigger.get().subject_mode;
        let subject = script.name.text.clone();
        let source = script.name.source.clone();
        let is_type = matches!(mode, SubjectMode::Type { .. });
        if subject.contains(' ') && !is_type {
            self.diagnostics.error(&source, msg::SCRIPT_SUBJECT_NO_SPACES, args(&[identifier]));
            return;
        }
        match (mode, subject.as_str()) {
            (SubjectMode::Name, _) | (SubjectMode::None, "_") => {}
            (SubjectMode::Type { global, .. }, "_") => {
                if !global {
                    self.diagnostics.error(&source, msg::SCRIPT_SUBJECT_NO_GLOBAL, args(&[identifier]));
                }
            }
            (SubjectMode::None, _) => self.diagnostics.error(&source, msg::SCRIPT_SUBJECT_ONLY_GLOBAL, args(&[identifier])),
            (SubjectMode::Type { category, .. }, s) if s.starts_with('_') => {
                if !category {
                    self.diagnostics.error(&source, msg::SCRIPT_SUBJECT_NO_CATEGORY, args(&[identifier]));
                } else {
                    let category_type = sv::CATEGORY;
                    self.resolve_subject(script, &s[1..], category_type);
                }
            }
            (SubjectMode::Type { ty, .. }, s) => self.resolve_subject(script, s, ty),
        }
    }

    fn resolve_subject(&mut self, script: &mut Script, subject: &str, ty: TypeId) {
        if ty == MAPZONE || ty == COORD {
            let packed = if ty == MAPZONE { self.parse_mapzone(script, subject) } else { self.parse_zone(script, subject) };
            let symbol = self.c.tables.add(Symbol::Basic { name: packed.to_string(), ty, is_protected: false });
            script.subject_reference = Some(symbol);
            return;
        }
        match self.c.tables.find(SymbolTables::ROOT, SymbolType::Basic(ty), subject) {
            Some(symbol) => script.subject_reference = Some(symbol),
            None => self.diagnostics.error(&script.name.source, msg::GENERIC_UNRESOLVED_SYMBOL, args(&[subject])),
        }
    }

    /// `level_mx_mz` → packed zone, `-1` on a reported error.
    fn parse_mapzone(&mut self, script: &Script, coord: &str) -> i32 {
        let src = &script.name.source;
        let Some([level, mx, mz]) = coord_parts(coord) else {
            self.diagnostics.error(src, "a mapzone subject must have the form `level_mx_mz`", vec![]);
            return -1;
        };
        if !(0..=255).contains(&mx) || !(0..=255).contains(&mz) {
            self.diagnostics.error(src, "invalid mapzone coordinate", vec![]);
        }
        if level != 0 {
            self.diagnostics.error(src, "a mapzone covers every level: use level `0`", vec![]);
            return -1;
        }
        let x = mx.wrapping_shl(6);
        let z = mz.wrapping_shl(6);
        (z & 0x3fff) | (x & 0x3fff).wrapping_shl(14)
    }

    /// `level_mx_mz_lx_lz` → packed zone, `-1` on a reported error.
    fn parse_zone(&mut self, script: &Script, coord: &str) -> i32 {
        let src = &script.name.source;
        let Some([level, mx, mz, lx, lz]) = coord_parts(coord) else {
            self.diagnostics.error(src, "a zone subject must have the form `level_mx_mz_lx_lz`", vec![]);
            return -1;
        };
        let out = |v: i32, max: i32| !(0..=max).contains(&v);
        if out(level, 3) || out(mx, 255) || out(mz, 255) || out(lx, 63) || out(lz, 63) {
            self.diagnostics.error(src, "invalid zone coordinate", vec![]);
        }
        if lx % 8 != 0 || lz % 8 != 0 {
            self.diagnostics.error(src, "local zone coordinates must be multiples of 8", vec![]);
            return -1;
        }
        let x = ((mx.wrapping_shl(6) | lx) >> 3) << 3;
        let z = ((mz.wrapping_shl(6) | lz) >> 3) << 3;
        (z & 0x3fff) | (x & 0x3fff).wrapping_shl(14) | (level & 0x3).wrapping_shl(28)
    }

    /// Returns the parameter's type.
    fn parameter(&mut self, p: &mut Parameter, trigger: Option<TriggerId>, scope: ScopeId) -> TypeId {
        let type_text = p.type_token.text.clone();
        let ty = if !self.features().procs && trigger != Some(COMMAND) {
            self.diagnostics.error(&p.source, msg::FEATURE_DISABLED_LOCAL, vec![]);
            Some(ERROR)
        } else if self.is_disabled_type_name(&type_text) {
            self.diagnostics.error(&p.source, msg::FEATURE_DISABLED_TYPE, args(&[&type_text]));
            Some(ERROR)
        } else {
            let ty = self.c.types.find_or_null(&type_text, true);
            if let Some(t) = ty
                && t != ERROR
                && trigger != Some(COMMAND)
                && !self.c.types[t].options.allow_parameter
            {
                let rep = self.c.types[t].representation.clone();
                self.diagnostics.error(&p.type_token.source, msg::LOCAL_PARAMETER_INVALID_TYPE, args(&[&rep]));
            }
            ty
        };
        if ty.is_none() {
            self.diagnostics.error(&p.source, msg::GENERIC_INVALID_TYPE, args(&[&type_text]));
        }
        let ty = ty.unwrap_or(ERROR);
        let symbol = self.c.tables.add(Symbol::LocalVariable { name: p.name.text.clone(), ty });
        if self.c.tables.insert(scope, SymbolType::LocalVariable, symbol) {
            self.c.tables.declared_at(symbol, &p.source);
        } else {
            let first = self.c.tables.find(scope, SymbolType::LocalVariable, &p.name.text).and_then(|s| self.c.tables.decl(s)).cloned();
            let d = self.diagnostics.emit(DiagnosticType::Error, &p.source, msg::SCRIPT_LOCAL_REDECLARATION, args(&[&p.name.text]));
            d.note(first.as_ref(), "first defined here");
        }
        p.symbol = Some(symbol);
        ty
    }
}

/// `a_b_câ€¦` as exactly `N` decimal i32 parts, or `None` (wrong count or a part
/// that is not a number).
fn coord_parts<const N: usize>(coord: &str) -> Option<[i32; N]> {
    let parts: Vec<i32> = coord.split('_').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    parts.try_into().ok()
}

/// A script's `[trigger,name]` header, where its errors and `defined here` notes point.
fn header(script: &Script) -> Loc {
    Loc { end_line: script.name.source.end_line, end_column: script.name.source.end_column + 1, ..script.source.clone() }
}
