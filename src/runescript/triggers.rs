//! The 156 server triggers in declaration order. `identifier` is the lowercased
//! constant name. Numeric ids are not unique (`INV_BUTTON1` and `IF_BUTTON1` are
//! both 149), so identity is [`TriggerId`], never `id`. PROC/LABEL allow all 24
//! pointers.

use crate::pointer::PointerType;
use crate::runescript::types;
use crate::trigger::{SubjectMode, TriggerId, TriggerType};

use crate::pointer::PointerType as P;

const fn t(
    identifier: &'static str,
    subject_mode: SubjectMode,
    allow_parameters: bool,
    allow_returns: bool,
    pointers: Option<&'static [PointerType]>,
) -> TriggerType {
    TriggerType { id: ares_shared::trigger::id(identifier), identifier, subject_mode, allow_parameters, allow_returns, pointers }
}

/// Every server trigger; `TriggerId(i + 1)` is entry `i` (0 is the command trigger).
pub static SERVER_TRIGGERS: [TriggerType; 156] = [
    t("proc", SubjectMode::Name, true, true, Some(PointerType::ALL)),
    t("label", SubjectMode::Name, true, false, Some(PointerType::ALL)),
    t("debugproc", SubjectMode::Name, true, false, Some(&[P::ActivePlayer])),
    t("apnpc1", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveNpc])),
    t("apnpc2", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveNpc])),
    t("apnpc3", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveNpc])),
    t("apnpc4", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveNpc])),
    t("apnpc5", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveNpc])),
    t("apnpcu", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::LastUseitem, P::LastUseslot, P::ActiveNpc])),
    t("apnpct", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveNpc])),
    t("opnpc1", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveNpc])),
    t("opnpc2", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveNpc])),
    t("opnpc3", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveNpc])),
    t("opnpc4", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveNpc])),
    t("opnpc5", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveNpc])),
    t("opnpcu", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::LastUseitem, P::LastUseslot, P::ActiveNpc])),
    t("opnpct", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveNpc])),
    t("ai_apnpc1", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveNpc2])),
    t("ai_apnpc2", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveNpc2])),
    t("ai_apnpc3", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveNpc2])),
    t("ai_apnpc4", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveNpc2])),
    t("ai_apnpc5", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveNpc2])),
    t("ai_opnpc1", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveNpc2])),
    t("ai_opnpc2", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveNpc2])),
    t("ai_opnpc3", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveNpc2])),
    t("ai_opnpc4", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveNpc2])),
    t("ai_opnpc5", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveNpc2])),
    t("apobj1", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveObj])),
    t("apobj2", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveObj])),
    t("apobj3", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveObj])),
    t("apobj4", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveObj])),
    t("apobj5", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveObj])),
    t("apobju", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::LastUseitem, P::LastUseslot, P::ActiveObj])),
    t("apobjt", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveObj])),
    t("opobj1", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveObj])),
    t("opobj2", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveObj])),
    t("opobj3", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveObj])),
    t("opobj4", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveObj])),
    t("opobj5", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveObj])),
    t("opobju", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::LastUseitem, P::LastUseslot, P::ActiveObj])),
    t("opobjt", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveObj])),
    t("ai_apobj1", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveObj])),
    t("ai_apobj2", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveObj])),
    t("ai_apobj3", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveObj])),
    t("ai_apobj4", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveObj])),
    t("ai_apobj5", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveObj])),
    t("ai_opobj1", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveObj])),
    t("ai_opobj2", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveObj])),
    t("ai_opobj3", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveObj])),
    t("ai_opobj4", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveObj])),
    t("ai_opobj5", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveObj])),
    t("aploc1", SubjectMode::Type { ty: types::LOC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveLoc])),
    t("aploc2", SubjectMode::Type { ty: types::LOC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveLoc])),
    t("aploc3", SubjectMode::Type { ty: types::LOC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveLoc])),
    t("aploc4", SubjectMode::Type { ty: types::LOC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveLoc])),
    t("aploc5", SubjectMode::Type { ty: types::LOC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveLoc])),
    t("aplocu", SubjectMode::Type { ty: types::LOC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::LastUseitem, P::LastUseslot, P::ActiveLoc])),
    t("aploct", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveLoc])),
    t("oploc1", SubjectMode::Type { ty: types::LOC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveLoc])),
    t("oploc2", SubjectMode::Type { ty: types::LOC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveLoc])),
    t("oploc3", SubjectMode::Type { ty: types::LOC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveLoc])),
    t("oploc4", SubjectMode::Type { ty: types::LOC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveLoc])),
    t("oploc5", SubjectMode::Type { ty: types::LOC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveLoc])),
    t("oplocu", SubjectMode::Type { ty: types::LOC, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::LastUseitem, P::LastUseslot, P::ActiveLoc])),
    t("oploct", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActiveLoc])),
    t("ai_aploc1", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveLoc])),
    t("ai_aploc2", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveLoc])),
    t("ai_aploc3", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveLoc])),
    t("ai_aploc4", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveLoc])),
    t("ai_aploc5", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveLoc])),
    t("ai_oploc1", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveLoc])),
    t("ai_oploc2", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveLoc])),
    t("ai_oploc3", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveLoc])),
    t("ai_oploc4", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveLoc])),
    t("ai_oploc5", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActiveLoc])),
    t("applayer1", SubjectMode::None, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActivePlayer2])),
    t("applayer2", SubjectMode::None, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActivePlayer2])),
    t("applayer3", SubjectMode::None, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActivePlayer2])),
    t("applayer4", SubjectMode::None, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActivePlayer2])),
    t("applayer5", SubjectMode::None, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActivePlayer2])),
    t("applayeru", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: false }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::LastUseitem, P::LastUseslot, P::ActivePlayer2])),
    t("applayert", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActivePlayer2])),
    t("opplayer1", SubjectMode::None, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActivePlayer2])),
    t("opplayer2", SubjectMode::None, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActivePlayer2])),
    t("opplayer3", SubjectMode::None, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActivePlayer2])),
    t("opplayer4", SubjectMode::None, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActivePlayer2])),
    t("opplayer5", SubjectMode::None, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActivePlayer2])),
    t("opplayeru", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: false }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::LastUseitem, P::LastUseslot, P::ActivePlayer2])),
    t("opplayert", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::ActivePlayer2])),
    t("ai_applayer1", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActivePlayer])),
    t("ai_applayer2", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActivePlayer])),
    t("ai_applayer3", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActivePlayer])),
    t("ai_applayer4", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActivePlayer])),
    t("ai_applayer5", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActivePlayer])),
    t("ai_opplayer1", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActivePlayer])),
    t("ai_opplayer2", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActivePlayer])),
    t("ai_opplayer3", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActivePlayer])),
    t("ai_opplayer4", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActivePlayer])),
    t("ai_opplayer5", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::ActivePlayer])),
    t("queue", SubjectMode::Name, true, false, Some(&[P::ActivePlayer, P::PActivePlayer])),
    t("ai_queue1", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue2", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue3", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue4", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue5", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue6", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue7", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue8", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue9", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue10", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue11", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue12", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue13", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue14", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue15", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue16", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue17", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue18", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue19", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("ai_queue20", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc, P::LastInt])),
    t("softtimer", SubjectMode::Name, true, false, Some(&[P::ActivePlayer])),
    t("timer", SubjectMode::Name, true, false, Some(&[P::ActivePlayer, P::PActivePlayer])),
    t("ai_timer", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc])),
    t("opheld1", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::LastItem, P::LastSlot, P::ActiveInvObj])),
    t("opheld2", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::LastItem, P::LastSlot, P::ActiveInvObj])),
    t("opheld3", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::LastItem, P::LastSlot, P::ActiveInvObj])),
    t("opheld4", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::LastItem, P::LastSlot, P::ActiveInvObj])),
    t("opheld5", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::LastItem, P::LastSlot, P::ActiveInvObj])),
    t("opheldu", SubjectMode::Type { ty: types::NAMEDOBJ, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::LastItem, P::LastSlot, P::ActiveInvObj, P::LastUseitem, P::LastUseslot])),
    t("opheldt", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer, P::LastItem, P::LastSlot, P::ActiveInvObj])),
    t("if_button", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::LastCom])),
    t("if_close", SubjectMode::Type { ty: types::INTERFACE, category: true, global: true }, false, false, Some(&[P::ActivePlayer])),
    t("inv_button1", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::LastItem, P::LastSlot, P::ActiveInvObj])),
    t("if_button1", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::LastItem, P::LastSlot])),
    t("inv_button2", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::LastItem, P::LastSlot, P::ActiveInvObj])),
    t("if_button2", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::LastItem, P::LastSlot])),
    t("inv_button3", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::LastItem, P::LastSlot, P::ActiveInvObj])),
    t("if_button3", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::LastItem, P::LastSlot])),
    t("inv_button4", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::LastItem, P::LastSlot, P::ActiveInvObj])),
    t("if_button4", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::LastItem, P::LastSlot])),
    t("inv_button5", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::LastItem, P::LastSlot, P::ActiveInvObj])),
    t("if_button5", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::LastItem, P::LastSlot])),
    t("inv_buttond", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::LastSlot, P::LastTargetslot])),
    t("if_buttond", SubjectMode::Type { ty: types::COMPONENT, category: false, global: false }, false, false, Some(&[P::ActivePlayer, P::LastSlot, P::LastTargetslot])),
    t("walktrigger", SubjectMode::Name, false, false, Some(&[P::ActivePlayer, P::PActivePlayer])),
    t("login", SubjectMode::None, false, false, Some(&[P::ActivePlayer, P::PActivePlayer])),
    t("logout", SubjectMode::None, false, false, Some(&[P::ActivePlayer, P::PActivePlayer])),
    t("tutorial", SubjectMode::None, false, false, Some(&[P::ActivePlayer, P::PActivePlayer])),
    t("advancestat", SubjectMode::Type { ty: types::STAT, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer])),
    t("mapzone", SubjectMode::Type { ty: crate::types::MAPZONE, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer])),
    t("mapzoneexit", SubjectMode::Type { ty: crate::types::MAPZONE, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer])),
    t("zone", SubjectMode::Type { ty: crate::types::COORD, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer])),
    t("zoneexit", SubjectMode::Type { ty: crate::types::COORD, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer])),
    t("changestat", SubjectMode::Type { ty: types::STAT, category: true, global: true }, false, false, Some(&[P::ActivePlayer, P::PActivePlayer])),
    t("ai_spawn", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc])),
    t("ai_despawn", SubjectMode::Type { ty: types::NPC, category: true, global: true }, false, false, Some(&[P::ActiveNpc])),
];

pub const PROC: TriggerId = TriggerId(1);
pub const LABEL: TriggerId = TriggerId(2);
pub const DEBUGPROC: TriggerId = TriggerId(3);
pub const QUEUE: TriggerId = TriggerId(100);
pub const SOFTTIMER: TriggerId = TriggerId(121);
pub const TIMER: TriggerId = TriggerId(122);
pub const IF_BUTTON: TriggerId = TriggerId(131);
pub const INV_BUTTON1: TriggerId = TriggerId(133);
pub const IF_BUTTON1: TriggerId = TriggerId(134);
pub const INV_BUTTON2: TriggerId = TriggerId(135);
pub const INV_BUTTON3: TriggerId = TriggerId(137);
pub const INV_BUTTON4: TriggerId = TriggerId(139);
pub const INV_BUTTON5: TriggerId = TriggerId(141);
pub const INV_BUTTOND: TriggerId = TriggerId(143);
pub const WALKTRIGGER: TriggerId = TriggerId(145);
pub const MAPZONE: TriggerId = TriggerId(150);
pub const AI_DESPAWN: TriggerId = TriggerId(156);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_ids_match_the_table() {
        for (id, name) in [
            (PROC, "proc"), (LABEL, "label"), (DEBUGPROC, "debugproc"), (QUEUE, "queue"), (SOFTTIMER, "softtimer"), (TIMER, "timer"),
            (IF_BUTTON, "if_button"), (INV_BUTTON1, "inv_button1"), (IF_BUTTON1, "if_button1"), (INV_BUTTON2, "inv_button2"),
            (INV_BUTTON3, "inv_button3"), (INV_BUTTON4, "inv_button4"), (INV_BUTTON5, "inv_button5"), (INV_BUTTOND, "inv_buttond"),
            (WALKTRIGGER, "walktrigger"), (MAPZONE, "mapzone"), (AI_DESPAWN, "ai_despawn"),
        ] {
            assert_eq!(id.get().identifier, name);
        }
    }
}
