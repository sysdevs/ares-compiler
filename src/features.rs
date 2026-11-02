//! Strict feature flags. Every flag defaults on except `topLevelDefOnly`,
//! which defaults off.

/// Deserialises from the config (camelCase keys, every key optional).
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StrictFeatureLevel {
    pub booleans: bool,
    pub procs: bool,
    pub macros: bool,
    pub enums: bool,
    pub structs: bool,
    pub dbtables: bool,
    pub logical_and: bool,
    pub calc: bool,
    pub relational_equals: bool,
    pub queue_typed: bool,
    pub top_level_def_only: bool,
    pub pointer_inversion: bool,
}

impl Default for StrictFeatureLevel {
    fn default() -> Self {
        Self {
            booleans: true,
            procs: true,
            macros: true,
            enums: true,
            structs: true,
            dbtables: true,
            logical_and: true,
            calc: true,
            relational_equals: true,
            queue_typed: true,
            top_level_def_only: false,
            pointer_inversion: true,
        }
    }
}

impl StrictFeatureLevel {
    /// Used by registration and type checking: the lowercased
    /// name minus an `array` suffix names a feature-disabled type.
    pub fn is_disabled_type_name(&self, text: &str) -> bool {
        let text = text.to_lowercase();
        let base = text.strip_suffix("array").unwrap_or(&text);
        (!self.booleans && base == "boolean")
            || (!self.enums && base == "enum")
            || (!self.structs && base == "struct")
            || (!self.dbtables && matches!(base, "dbtable" | "dbrow" | "dbcolumn"))
    }

    /// Used by type checking.
    pub fn is_disabled_command_name(&self, name: &str) -> bool {
        (!self.enums && name == "enum")
            || (!self.structs && name == "struct_param")
            || (!self.dbtables
                && matches!(name, "db_find" | "db_find_refine" | "db_find_with_count" | "db_find_refine_with_count" | "db_getfield"))
    }
}
