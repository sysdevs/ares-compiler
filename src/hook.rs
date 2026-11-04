//! Client hook strings: the first argument of the `cc_seton*`/`if_seton*`
//! commands, `script(args){triggers}` as Jagex writes them. The call part is
//! parsed as an expression; the trigger list names vars, invs or stats.

/// Splits `script(args){a, b}` into the call text and the trigger names
/// (none without a `{...}` suffix).
pub fn split(text: &str) -> Result<(String, Vec<String>), String> {
    let Some(open) = text.find('{') else {
        if text.contains('}') {
            return Err("unmatched `}` in hook".into());
        }
        return Ok((text.trim().to_owned(), Vec::new()));
    };
    let Some(list) = text[open + 1..].strip_suffix('}') else {
        return Err("a hook's trigger list must end it: `script(args){triggers}`".into());
    };
    if list.contains(['{', '}']) {
        return Err("a hook has one trigger list".into());
    }
    let triggers = list.split(',').map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned).collect();
    Ok((text[..open].trim().to_owned(), triggers))
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn splits_call_and_triggers() {
        assert_eq!(split("draw($c){worn}"), Ok(("draw($c)".into(), vec!["worn".into()])));
        assert_eq!(split("draw"), Ok(("draw".into(), vec![])));
        assert_eq!(split("a(1){x, y}"), Ok(("a(1)".into(), vec!["x".into(), "y".into()])));
        assert!(split("a{").is_err());
        assert!(split("a}").is_err());
    }
}
