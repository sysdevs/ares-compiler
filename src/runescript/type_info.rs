//! One table of `symbols.json`. Every field is required. Maps keep file order;
//! id tables are read through [`CompilerTypeInfo::ids`].

use indexmap::IndexMap;
use serde::Deserialize;

use crate::FatalError;

#[derive(Clone, Debug, Deserialize)]
pub struct CompilerTypeInfo {
    pub max: i32,
    pub map: IndexMap<String, String>,
    pub vartype: IndexMap<String, String>,
    pub protect: IndexMap<String, bool>,
    pub require: IndexMap<String, String>,
    pub require2: IndexMap<String, String>,
    pub conditional: IndexMap<String, bool>,
    pub set: IndexMap<String, String>,
    pub set2: IndexMap<String, String>,
    pub corrupt: IndexMap<String, String>,
    pub corrupt2: IndexMap<String, String>,
}

impl CompilerTypeInfo {
    /// `map` as `(id, key, name)` sorted by id (stable, so a repeated id keeps
    /// file order). Every key must be a decimal `i32`.
    pub fn ids(&self) -> Result<Vec<(i32, &str, &str)>, FatalError> {
        let mut v = self
            .map
            .iter()
            .map(|(k, name)| match k.parse::<i32>() {
                Ok(id) => Ok((id, k.as_str(), name.as_str())),
                Err(_) => Err(FatalError(format!("Invalid symbol id {k:?} for {name}."))),
            })
            .collect::<Result<Vec<_>, _>>()?;
        v.sort_by_key(|&(id, _, _)| id);
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_sorted_and_must_be_decimal() {
        let info = |map: &str| -> CompilerTypeInfo {
            serde_json::from_str(&format!(
                r#"{{"max":0,"map":{map},"vartype":{{}},"protect":{{}},"require":{{}},"require2":{{}},"conditional":{{}},"set":{{}},"set2":{{}},"corrupt":{{}},"corrupt2":{{}}}}"#
            ))
            .unwrap()
        };
        let t = info(r#"{"10":"b","2":"a","-1":"m"}"#);
        let ids: Vec<_> = t.ids().unwrap().into_iter().map(|(id, _, n)| (id, n)).collect();
        assert_eq!(ids, [(-1, "m"), (2, "a"), (10, "b")]);
        assert!(info(r#"{"0x1F":"a"}"#).ids().is_err());
        assert!(info(r#"{"12abc":"a"}"#).ids().is_err());
    }

    #[test]
    fn every_fixture_symbols_json_loads() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        for case in std::fs::read_dir(dir).unwrap() {
            let text = std::fs::read_to_string(case.unwrap().path().join("symbols.json")).unwrap();
            let tables: IndexMap<String, CompilerTypeInfo> = serde_json::from_str(&text).unwrap();
            assert!(tables.contains_key("command") && tables.contains_key("runescript"));
        }
    }
}
