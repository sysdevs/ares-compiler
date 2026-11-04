//! Trigger types, subject modes and the name lookup.

use crate::compiler::Dialect;
use crate::pointer::PointerType;
use crate::runescript::client_triggers::{self, CLIENT_TRIGGERS};
use crate::runescript::triggers::{self as server, SERVER_TRIGGERS};
use crate::types::TypeId;

/// Identity of a trigger (numeric ids are not unique):
/// 0 is [`COMMAND`], `n` is `SERVER_TRIGGERS[n - 1]`, `1000 + n` is `CLIENT_TRIGGERS[n]`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TriggerId(pub(crate) u32);

impl TriggerId {
    pub fn get(self) -> &'static TriggerType {
        match self.0 {
            0 => &COMMAND_TRIGGER,
            n if n >= client_triggers::FIRST => &CLIENT_TRIGGERS[(n - client_triggers::FIRST) as usize],
            n => &SERVER_TRIGGERS[n as usize - 1],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SubjectMode {
    None,
    Name,
    Type { ty: TypeId, category: bool, global: bool },
}

#[derive(Debug)]
pub struct TriggerType {
    /// Not unique; see [`TriggerId`].
    pub id: i32,
    pub identifier: &'static str,
    pub subject_mode: SubjectMode,
    pub allow_parameters: bool,
    pub allow_returns: bool,
    pub pointers: Option<&'static [PointerType]>,
}

/// The trigger of command scripts.
pub const COMMAND: TriggerId = TriggerId(0);

static COMMAND_TRIGGER: TriggerType = TriggerType {
    id: -1,
    identifier: "command",
    subject_mode: SubjectMode::Name,
    allow_parameters: true,
    allow_returns: true,
    pointers: None,
};

/// The trigger named `name` in `dialect`: `command` and the server triggers, or
/// `command`, `proc` and the client triggers.
pub fn by_name(name: &str, dialect: Dialect) -> Option<TriggerId> {
    if name == COMMAND_TRIGGER.identifier {
        return Some(COMMAND);
    }
    match dialect {
        Dialect::Server => SERVER_TRIGGERS.iter().position(|t| t.identifier == name).map(|i| TriggerId(i as u32 + 1)),
        Dialect::Client if name == server::PROC.get().identifier => Some(server::PROC),
        Dialect::Client => CLIENT_TRIGGERS.iter().position(|t| t.identifier == name).map(|i| TriggerId(i as u32 + client_triggers::FIRST)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runescript::triggers;
    use crate::compiler::Dialect::Server;

    #[test]
    fn lookup_and_static_table() {
        assert_eq!(by_name("proc", Server), Some(triggers::PROC));
        assert_eq!(by_name("ai_despawn", Server), Some(triggers::AI_DESPAWN));
        assert_eq!(by_name("command", Server).unwrap().get().id, -1);
        assert_eq!(by_name("nope", Server), None);
        // Shared numeric id, distinct identity.
        assert_eq!(triggers::INV_BUTTON1.get().id, triggers::IF_BUTTON1.get().id);
        assert_ne!(triggers::INV_BUTTON1, triggers::IF_BUTTON1);
        assert_eq!(triggers::LABEL.get().pointers.unwrap().len(), 26);
    }

    #[test]
    fn client_dialect_triggers() {
        use crate::compiler::Dialect::{Client, Server};
        use crate::runescript::client_triggers::CLIENTSCRIPT;
        assert_eq!(by_name("clientscript", Client), Some(CLIENTSCRIPT));
        assert_eq!(CLIENTSCRIPT.get().id, 10);
        assert!(!CLIENTSCRIPT.get().allow_returns && CLIENTSCRIPT.get().allow_parameters);
        assert_eq!(by_name("label", Client), None);
        assert_eq!(by_name("clientscript", Server), None);
        assert_eq!(by_name("proc", Client), Some(triggers::PROC));
        assert_eq!(by_name("command", Client), Some(COMMAND));
    }
}
