//! The client dialect's own triggers. `proc` and `command` are shared with the
//! server; `clientscript` is the only new one. `TriggerId(1000 + i)` is entry `i`.

use crate::trigger::{SubjectMode, TriggerId, TriggerType};

pub static CLIENT_TRIGGERS: [TriggerType; 1] = [TriggerType {
    id: 10,
    identifier: "clientscript",
    subject_mode: SubjectMode::Name,
    allow_parameters: true,
    allow_returns: false,
    pointers: None,
}];

/// The first client trigger's [`TriggerId`].
pub(crate) const FIRST: u32 = 1000;

pub const CLIENTSCRIPT: TriggerId = TriggerId(FIRST);
