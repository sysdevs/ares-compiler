//! Symbol → id for the writer, from the `symbols.json` keys. Every `put*`
//! keeps the first insert.

use std::collections::HashMap;

use crate::symbol::{Symbol, SymbolId, SymbolTables};
use crate::trigger::COMMAND;
use crate::FatalError;

#[derive(Default, Debug)]
pub struct SymbolMapper {
    commands: HashMap<String, i32>,
    scripts: HashMap<String, i32>,
    symbols: HashMap<SymbolId, i32>,
}

impl SymbolMapper {
    pub fn put_symbol(&mut self, id: i32, symbol: SymbolId) {
        self.symbols.entry(symbol).or_insert(id);
    }

    pub fn put_command(&mut self, id: i32, name: &str) {
        self.commands.entry(name.to_owned()).or_insert(id);
    }

    pub fn put_script(&mut self, id: i32, name: &str) {
        self.scripts.entry(name.to_owned()).or_insert(id);
    }

    /// Commands drop everything up to the first `.`, other scripts look up
    /// `[trigger,name]`; both give `-1` when unknown (written as is). Any other
    /// unknown symbol is an error.
    pub fn get(&self, tables: &SymbolTables, symbol: SymbolId) -> Result<i32, FatalError> {
        match &tables[symbol] {
            Symbol::Script { trigger, name, .. } if *trigger == COMMAND => {
                let name = name.find('.').map_or(name.as_str(), |i| &name[i + 1..]);
                Ok(self.commands.get(name).copied().unwrap_or(-1))
            }
            Symbol::Script { trigger, name, .. } => {
                Ok(self.scripts.get(&format!("[{},{name}]", trigger.get().identifier)).copied().unwrap_or(-1))
            }
            other => self.symbols.get(&symbol).copied().ok_or_else(|| FatalError(format!("Unable to find id for '{}'.", other.name()))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runescript::triggers;
    use crate::types::{INT, UNIT};

    #[test]
    fn first_insert_wins_and_lookup_rules() {
        let mut t = SymbolTables::default();
        let mut m = SymbolMapper::default();
        m.put_command(5, "mes");
        m.put_command(6, "mes");
        m.put_script(9, "[proc,foo]");
        let cmd = |t: &mut SymbolTables, name: &str| {
            t.add(Symbol::Script { trigger: COMMAND, name: name.into(), parameters: UNIT, returns: UNIT })
        };
        let (mes, dot_mes, other) = (cmd(&mut t, "mes"), cmd(&mut t, ".mes"), cmd(&mut t, "nope"));
        assert_eq!(m.get(&t, mes).unwrap(), 5);
        assert_eq!(m.get(&t, dot_mes).unwrap(), 5);
        assert_eq!(m.get(&t, other).unwrap(), -1);
        let proc_ = t.add(Symbol::Script { trigger: triggers::PROC, name: "foo".into(), parameters: UNIT, returns: UNIT });
        let missing = t.add(Symbol::Script { trigger: triggers::LABEL, name: "foo".into(), parameters: UNIT, returns: UNIT });
        assert_eq!(m.get(&t, proc_).unwrap(), 9);
        assert_eq!(m.get(&t, missing).unwrap(), -1);
        let b = t.add(Symbol::Basic { name: "x".into(), ty: INT, is_protected: false });
        assert!(m.get(&t, b).is_err());
        m.put_symbol(3, b);
        m.put_symbol(4, b);
        assert_eq!(m.get(&t, b).unwrap(), 3);
    }
}
