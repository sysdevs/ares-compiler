//! Compiler diagnostics. Each stage returns its [`Diagnostics`] and the caller
//! decides what to do with errors.

pub mod messages;
pub mod render;

use serde::{Deserialize, Serialize};

use crate::ast::Loc;

/// Serialised in SCREAMING_SNAKE_CASE.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiagnosticType {
    Info,
    Hint,
    Warning,
    Error,
    SyntaxError,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Diagnostic {
    pub kind: DiagnosticType,
    pub source: Loc,
    /// A `messages` template (or free text), `%s` placeholders unfilled.
    pub message: String,
    /// Message args, already stringified.
    pub args: Vec<String>,
    /// Text under the primary span.
    pub label: Option<String>,
    pub notes: Vec<Note>,
}

/// A `note:` (or `help:`) line, with its own snippet when it has a location.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Note {
    pub help: bool,
    pub source: Option<Loc>,
    pub message: String,
}

impl Diagnostic {
    pub fn label(&mut self, text: impl Into<String>) -> &mut Self {
        self.label = Some(text.into());
        self
    }

    pub fn note(&mut self, source: Option<&Loc>, message: impl Into<String>) -> &mut Self {
        self.notes.push(Note { help: false, source: source.cloned(), message: message.into() });
        self
    }

    pub fn help(&mut self, message: impl Into<String>) -> &mut Self {
        self.notes.push(Note { help: true, source: None, message: message.into() });
        self
    }

    pub fn is_error(&self) -> bool {
        matches!(self.kind, DiagnosticType::Error | DiagnosticType::SyntaxError)
    }

    /// Fills the `%s` template: with no args the template is returned
    /// as is (so `%%` stays `%%`); otherwise `%s` takes the next arg, `%%` is `%`,
    /// and unused args are appended separated by spaces.
    pub fn formatted(&self) -> String {
        if self.args.is_empty() {
            return self.message.clone();
        }
        let mut out = String::new();
        let mut args = self.args.iter();
        let mut chars = self.message.chars().peekable();
        while let Some(c) = chars.next() {
            match (c, chars.peek()) {
                ('%', Some('s')) => match args.next() {
                    Some(a) => {
                        chars.next();
                        out.push_str(a);
                    }
                    None => out.push('%'),
                },
                ('%', Some('%')) => {
                    chars.next();
                    out.push('%');
                }
                _ => out.push(c),
            }
        }
        for a in args {
            out.push(' ');
            out.push_str(a);
        }
        out
    }
}

/// One per compiler stage.
#[derive(Clone, Default, Debug)]
pub struct Diagnostics {
    pub list: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn report(&mut self, kind: DiagnosticType, source: &Loc, message: &str, args: Vec<String>) {
        self.emit(kind, source, message, args);
    }

    /// [`report`](Self::report), returning the diagnostic to add a label or notes to.
    pub fn emit(&mut self, kind: DiagnosticType, source: &Loc, message: &str, args: Vec<String>) -> &mut Diagnostic {
        self.list.push(Diagnostic { kind, source: source.clone(), message: message.to_owned(), args, label: None, notes: Vec::new() });
        self.list.last_mut().unwrap()
    }

    pub fn error(&mut self, source: &Loc, message: &str, args: Vec<String>) {
        self.report(DiagnosticType::Error, source, message, args);
    }

    pub fn has_errors(&self) -> bool {
        self.list.iter().any(Diagnostic::is_error)
    }
}

/// The candidate closest to `name` by edit distance, if close enough to be a
/// likely typo (rustc's cutoff: a third of the length).
pub fn closest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let limit = (name.chars().count() / 3).max(1);
    candidates.into_iter().filter(|c| *c != name).map(|c| (edit_distance(name, c), c)).filter(|(d, _)| *d <= limit).min().map(|(_, c)| c)
}

/// Optimal string alignment distance: Levenshtein where swapping two adjacent
/// chars counts as one edit.
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut d = vec![vec![0; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() {
        for j in 0..=b.len() {
            d[i][j] = match (i, j) {
                (0, j) => j,
                (i, 0) => i,
                _ => {
                    let mut v = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1]));
                    if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                        v = v.min(d[i - 2][j - 2] + 1);
                    }
                    v
                }
            };
        }
    }
    d[a.len()][b.len()]
}

/// Template arguments for [`Diagnostics::error`] and friends.
pub(crate) fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(message: &str, args: &[&str]) -> String {
        let source = Loc { name: "f".into(), line: 1, column: 1, end_line: 1, end_column: 1 };
        Diagnostic { kind: DiagnosticType::Error, source, message: message.into(), args: args.iter().map(|s| s.to_string()).collect(), label: None, notes: vec![] }
            .formatted()
    }

    #[test]
    fn formats_templates() {
        assert_eq!(d(messages::SCRIPT_REDECLARATION, &["proc", "a"]), "`[proc,a]` is defined multiple times");
        assert_eq!(d(messages::GAME_REFERENCE_UNRESOLVED, &["x"]), "cannot find game variable `%x`");
        assert_eq!(d("100%% %s", &[]), "100%% %s", "no args: untouched");
        assert_eq!(d("%s and %s", &["a"]), "a and %s");
        assert_eq!(d("%s", &["a", "b"]), "a b");
    }

    #[test]
    fn closest_name() {
        let names = ["give_item", "take_item", "stat_add", "x"];
        assert_eq!(closest("giv_item", names), Some("give_item"));
        assert_eq!(closest("stat_ad", names), Some("stat_add"));
        assert_eq!(closest("nothing_like_it", names), None);
        assert_eq!(closest("y", names), Some("x"), "short names allow one edit");
        assert_eq!(closest("cuont", ["count"]), Some("count"), "a swap is one edit");
        assert_eq!(closest("x", names), None, "an exact match is not a suggestion");
    }
}
