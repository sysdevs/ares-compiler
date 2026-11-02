//! rustc-style text rendering of diagnostics: the source line with the span
//! underlined, then `note:`/`help:` lines.

use annotate_snippets::{Annotation, AnnotationKind, Group, Level, Renderer, Snippet};

use super::{Diagnostic, DiagnosticType};
use crate::ast::Loc;
use crate::preprocess::MacroExpansionSpan;

/// One compiled file as read from disk, with where macro expansions landed
/// (diagnostic locations are in expanded coordinates).
#[derive(Clone, Debug)]
pub struct SourceFile {
    pub name: String,
    pub text: String,
    pub expansions: Vec<MacroExpansionSpan>,
}

/// Every diagnostic, then rustc's `could not compile` line when any is an error.
pub fn render_all<'a>(diagnostics: impl IntoIterator<Item = &'a Diagnostic>, sources: &[SourceFile]) -> String {
    let mut out = String::new();
    let mut errors = 0;
    for d in diagnostics {
        errors += usize::from(d.is_error());
        out.push_str(&render(d, sources));
        out.push_str("\n\n");
    }
    if errors > 0 {
        let s = if errors == 1 { "" } else { "s" };
        let title = Level::ERROR.primary_title(format!("could not compile due to {errors} previous error{s}"));
        out.push_str(&Renderer::plain().render(&[Group::with_title(title)]));
        out.push('\n');
    }
    out
}

pub fn render(d: &Diagnostic, sources: &[SourceFile]) -> String {
    let level = match d.kind {
        DiagnosticType::Error | DiagnosticType::SyntaxError => Level::ERROR,
        DiagnosticType::Warning => Level::WARNING,
        DiagnosticType::Info => Level::INFO,
        DiagnosticType::Hint => Level::NOTE,
    };
    let mut main = Group::with_title(level.primary_title(d.formatted()));
    let mut expansion = None;
    if let Some((s, e)) = snippet(&d.source, d.label.as_deref(), sources) {
        main = main.element(s);
        expansion = e;
    }
    let mut groups = Vec::new();
    for n in &d.notes {
        let level = if n.help { Level::HELP } else { Level::NOTE };
        match n.source.as_ref().and_then(|loc| snippet(loc, None, sources)) {
            Some((s, _)) => groups.push(Group::with_title(level.secondary_title(n.message.as_str())).element(s)),
            None => main = main.element(level.message(n.message.as_str())),
        }
    }
    if let Some(e) = &expansion {
        main = main.element(Level::NOTE.message(e.as_str()));
    }
    let mut report = vec![main];
    report.extend(groups);
    Renderer::plain().render(&report).to_string()
}

/// The file's snippet with `loc` underlined (mapped out of macro expansions),
/// plus a note naming the macro when `loc` fell inside one.
fn snippet<'a>(loc: &Loc, label: Option<&'a str>, sources: &'a [SourceFile]) -> Option<(Snippet<'a, Annotation<'a>>, Option<String>)> {
    let file = sources.iter().find(|f| f.name == loc.name)?;
    let (start, end, inside) = to_raw(loc, &file.expansions);
    let note = inside.map(|s| format!("this error originates in the macro `#{}`", s.macro_name));
    let text = file.text.as_str();
    let lo = offset(text, start.0, start.1)?;
    let hi = offset(text, end.0, end.1).filter(|hi| *hi > lo).unwrap_or_else(|| widen(text, lo));
    let mut ann = AnnotationKind::Primary.span(lo..hi);
    if let Some(l) = label {
        ann = ann.label(l);
    }
    Some((Snippet::source(text).path(file.name.as_str()).annotation(ann), note))
}

/// A point span covers the token there (the run up to whitespace), at least one char.
fn widen(text: &str, lo: usize) -> usize {
    let rest = &text[lo..];
    match rest.find(char::is_whitespace).unwrap_or(rest.len()) {
        0 => lo + rest.chars().next().map_or(0, char::len_utf8),
        len => lo + len,
    }
}

/// Byte offset of 1-based `(line, column)`, columns counting chars with `\r`
/// skipped as the lexer does. Past the line's end clamps to it.
fn offset(text: &str, line: i32, column: i32) -> Option<usize> {
    let mut pos = 0;
    for _ in 1..line {
        pos += text[pos..].find('\n')? + 1;
    }
    let line_end = text[pos..].find('\n').map_or(text.len(), |i| pos + i);
    let mut col = 1;
    for (i, c) in text[pos..line_end].char_indices() {
        if col >= column {
            return Some(pos + i);
        }
        if c != '\r' {
            col += 1;
        }
    }
    Some(line_end)
}

/// Expanded `(line, column)` → raw. Inside an expansion it is the call site
/// (and the span is returned); past one, it shifts by where the call ended.
fn map_point(line: i32, column: i32, spans: &[MacroExpansionSpan]) -> ((i32, i32), Option<&MacroExpansionSpan>) {
    let at = (line as u32, column as u32);
    let mut raw = (line as i64, column as i64);
    for s in spans {
        if at < (s.start_line, s.start_column) {
            break;
        }
        if at < (s.end_line, s.end_column_exclusive) {
            return ((s.call_site_line as i32, s.call_site_column as i32), Some(s));
        }
        // Expansions are collapsed onto one output line, so only that line's
        // later columns shift by the call's length; later lines shift by the
        // lines a multi-line call took.
        raw.0 = at.0 as i64 + s.call_end_line as i64 - s.end_line as i64;
        if at.0 == s.end_line {
            raw.1 = at.1 as i64 - s.end_column_exclusive as i64 + s.call_end_column as i64;
        }
    }
    ((raw.0 as i32, raw.1 as i32), None)
}

/// A whole `Loc` in raw coordinates; one touching an expansion becomes its
/// call site (a point, widened when rendered).
fn to_raw<'a>(loc: &Loc, spans: &'a [MacroExpansionSpan]) -> ((i32, i32), (i32, i32), Option<&'a MacroExpansionSpan>) {
    let (start, a) = map_point(loc.line, loc.column, spans);
    let (end, b) = map_point(loc.end_line, loc.end_column, spans);
    match a.or(b) {
        Some(s) => {
            let p = (s.call_site_line as i32, s.call_site_column as i32);
            (p, p, Some(s))
        }
        None => (start, end, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preprocess::{expand_source_with_map, parse_macro_file, MacroRegistry};

    #[test]
    fn maps_expanded_positions_back() {
        let mut registry = MacroRegistry::default();
        let mut diags = Vec::new();
        parse_macro_file("m.macro", "#define TWO, a\n$x = a;\n$y = a;\n#enddef\n", &mut registry, &mut diags);
        let raw = "x = #TWO(\n  1); after\nnext\n";
        let (expanded, spans) = expand_source_with_map(raw, &registry, &mut diags, "f.rs2");
        assert!(diags.is_empty(), "{diags:?}");
        let at = |needle: &str| {
            let i = expanded.find(needle).unwrap();
            let line = expanded[..i].matches('\n').count() as i32 + 1;
            let col = expanded[..i].rsplit('\n').next().unwrap().chars().count() as i32 + 1;
            map_point(line, col, &spans)
        };
        assert_eq!(at("after"), ((2, 7), None), "same line as the expansion: column shifted\n{expanded:?}\n{spans:?}");
        assert_eq!(at("next"), ((3, 1), None), "multi-line call: later lines shift");
        let (p, s) = at("$y");
        assert_eq!((p, s.map(|s| s.macro_name.as_str())), ((1, 5), Some("TWO")), "inside: the call site");
    }

    #[test]
    fn offsets_and_widening() {
        let text = "ab\r\ncd ef\n";
        assert_eq!(offset(text, 2, 4), Some(7));
        assert_eq!(offset(text, 1, 9), Some(3), "clamps to the line end");
        assert_eq!(&text[7..widen(text, 7)], "ef");
    }
}
