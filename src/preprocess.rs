//! The macro preprocessor: `#define NAME, args` … `#enddef` blocks in `.macro`
//! files, expanded at `#NAME` / `#NAME(args)` call sites in `.rs2` sources.
//!
//! Text is processed as `Vec<char>`, so every index, column and length counts
//! chars; whitespace is [`char::is_whitespace`].
//! Rules worth knowing:
//! - a source with no `#` is returned untouched (CRLF kept); otherwise `\r\n`
//!   becomes `\n` outside strings, chars and comments;
//! - an expansion is collapsed onto the call-site line (newlines → spaces,
//!   line comments dropped) and swallows a trailing `;` after the call;
//! - the first definition of a name wins; a redefinition is an error.

use std::collections::HashMap;

use serde::Serialize;

pub struct MacroDefinition {
    pub name: String,
    body: Vec<char>,
    pub file: String,
    pub line: u32,
    args: Vec<Vec<char>>,
}

impl MacroDefinition {
    pub fn body(&self) -> String {
        s(&self.body)
    }

    pub fn args(&self) -> Vec<String> {
        self.args.iter().map(|a| s(a)).collect()
    }
}

#[derive(Default)]
pub struct MacroRegistry {
    pub macros: HashMap<String, MacroDefinition>,
}

/// Where an expansion landed in the output, and what produced it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroExpansionSpan {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub call_site_line: u32,
    pub call_site_column: u32,
    /// Raw input position just past the call (and its `;`).
    pub call_end_line: u32,
    pub call_end_column: u32,
    /// Output column just past the expansion.
    pub end_column_exclusive: u32,
    pub arg_values: Vec<String>,
    pub macro_name: String,
    pub macro_file: String,
    pub macro_line: u32,
}

/// An `ERROR` diagnostic reported by the preprocessor.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MacroDiagnostic {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
}

fn u(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn s(chars: &[char]) -> String {
    chars.iter().collect()
}

fn is_ws(c: char) -> bool {
    c.is_whitespace()
}

/// `str::trim` on a char slice.
fn trim(units: &[char]) -> &[char] {
    trim_end(&units[units.iter().position(|&c| !is_ws(c)).unwrap_or(units.len())..])
}

fn trim_end(units: &[char]) -> &[char] {
    &units[..units.iter().rposition(|&c| !is_ws(c)).map_or(0, |i| i + 1)]
}

/// `[0-9A-Za-z_+.:]`.
fn is_identifier_char(c: char) -> bool {
    matches!(c, '0'..='9' | 'A'..='Z' | 'a'..='z' | '_' | '+' | '.' | ':')
}

fn read_identifier(source: &[char], start: usize) -> Option<&[char]> {
    let end = source[start.min(source.len())..].iter().position(|&ch| !is_identifier_char(ch)).map_or(source.len(), |n| start + n);
    (end > start).then(|| &source[start..end])
}

fn skip_inline_whitespace(source: &[char], mut index: usize) -> usize {
    while index < source.len() && matches!(source[index], ' ' | '\t' | '\r' | '\n') {
        index += 1;
    }
    index
}

/// `^\s*#define\s+([a-zA-Z0-9_+.:]+)\s*,(.*)$` → (name, rest); a rest
/// containing `\r` or `\n` fails.
fn match_define(line: &[char]) -> Option<(&[char], &[char])> {
    let mut i = line.iter().position(|&ch| !is_ws(ch)).unwrap_or(line.len());
    let kw = u("#define");
    if !line[i..].starts_with(&kw) {
        return None;
    }
    i += kw.len();
    let ws = line[i..].iter().take_while(|&&ch| is_ws(ch)).count();
    if ws == 0 {
        return None;
    }
    i += ws;
    let name = read_identifier(line, i)?;
    i += name.len();
    i += line[i..].iter().take_while(|&&ch| is_ws(ch)).count();
    if line.get(i) != Some(&',') {
        return None;
    }
    let rest = &line[i + 1..];
    if rest.iter().any(|&ch| matches!(ch, '\n' | '\r')) {
        return None;
    }
    Some((name, rest))
}

/// `^\s*#enddef\b`.
fn match_enddef(line: &[char]) -> bool {
    let i = line.iter().position(|&ch| !is_ws(ch)).unwrap_or(line.len());
    let kw = u("#enddef");
    line[i..].starts_with(&kw)
        && line.get(i + kw.len()).is_none_or(|&ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
}

/// Drops a `//` comment outside strings/chars, then trailing whitespace.
fn strip_line_comment(line: &[char]) -> &[char] {
    let (mut in_string, mut in_char) = (false, false);
    let mut i = 0;
    while i + 1 < line.len() {
        let (current, next) = (line[i], line[i + 1]);
        if current == '\\' {
            i += 2;
            continue;
        }
        if in_string {
            in_string = current != '"';
        } else if in_char {
            in_char = current != '\'';
        } else if current == '"' {
            in_string = true;
        } else if current == '\'' {
            in_char = true;
        } else if current == '/' && next == '/' {
            return trim_end(&line[..i]);
        }
        i += 1;
    }
    trim_end(line)
}

fn report(diagnostics: &mut Vec<MacroDiagnostic>, file: &str, line: u32, column: u32, message: String) {
    diagnostics.push(MacroDiagnostic { file: file.to_owned(), line, column, message });
}

/// Registers every `#define … #enddef` block of a `.macro` file.
pub fn parse_macro_file(file_path: &str, source: &str, registry: &mut MacroRegistry, diagnostics: &mut Vec<MacroDiagnostic>) {
    let source = u(source);
    // Lines end at `\n` or `\r\n`.
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, &ch) in source.iter().enumerate() {
        if ch == '\n' {
            let end = if i > start && source[i - 1] == '\r' { i - 1 } else { i };
            lines.push(&source[start..end]);
            start = i + 1;
        }
    }
    lines.push(&source[start..]);

    // The open `#define` (args parsed up front; they do not depend on the body)
    // and its raw body lines.
    let mut current: Option<(MacroDefinition, Vec<&[char]>)> = None;
    for (index, &raw_line) in lines.iter().enumerate() {
        let line = strip_line_comment(raw_line);
        if let Some((_, body)) = &mut current {
            if !match_enddef(line) {
                body.push(raw_line);
                continue;
            }
            let (mut def, body) = current.take().unwrap();
            if registry.macros.contains_key(&def.name) {
                report(diagnostics, file_path, def.line, 1, format!("macro `#{}` is defined multiple times", def.name));
            } else {
                def.body = body.join(&'\n');
                registry.macros.insert(def.name.clone(), def);
            }
            continue;
        }
        if match_enddef(line) {
            report(diagnostics, file_path, index as u32 + 1, 1, "`#enddef` without a matching `#define`".into());
            continue;
        }
        if let Some((name, rest)) = match_define(line) {
            let rest = trim(rest);
            let args = if rest.is_empty() {
                Vec::new()
            } else {
                rest.split(|&ch| ch == ',').map(|a| trim(a).to_vec()).filter(|a| !a.is_empty()).collect()
            };
            let def = MacroDefinition { name: s(name), body: Vec::new(), file: file_path.to_owned(), line: index as u32 + 1, args };
            current = Some((def, Vec::new()));
        }
    }
    if let Some((def, _)) = current {
        report(diagnostics, file_path, def.line, 1, format!("macro `#{}` is missing its closing `#enddef`", def.name));
    }
}

/// Expands every macro call of a source, returning the text and where each expansion landed.
pub fn expand_source_with_map(
    source: &str,
    registry: &MacroRegistry,
    diagnostics: &mut Vec<MacroDiagnostic>,
    source_name: &str,
) -> (String, Vec<MacroExpansionSpan>) {
    if !source.contains('#') {
        return (source.to_owned(), Vec::new());
    }
    let (text, spans) = expand(&u(source), registry, diagnostics, source_name, &[], false);
    (s(&text), spans)
}

struct Expander<'a> {
    source: &'a [char],
    out: Vec<char>,
    index: usize,
    collapse: bool,
    output_line: u32,
    output_column: u32,
    input_line: u32,
    input_column: u32,
}

impl Expander<'_> {
    fn append(&mut self, text: &[char]) {
        for &ch in text {
            if ch == '\n' {
                self.output_line += 1;
                self.output_column = 1;
            } else {
                self.output_column += 1;
            }
        }
        self.out.extend_from_slice(text);
    }

    fn newline(&mut self) {
        self.append(if self.collapse { &[' '] } else { &['\n'] });
    }

    fn advance_input(&mut self, ch: char) {
        if ch == '\n' {
            self.input_line += 1;
            self.input_column = 1;
        } else if ch != '\r' {
            self.input_column += 1;
        }
    }

    /// Copies `n` chars to the output and advances past them.
    fn copy(&mut self, n: usize) {
        let src = self.source;
        let end = (self.index + n).min(src.len());
        self.append(&src[self.index..end]);
        self.skip_to(end);
    }

    /// Advances input position to `end` without output.
    fn skip_to(&mut self, end: usize) {
        for i in self.index..end {
            self.advance_input(self.source[i]);
        }
        self.index = end;
    }
}

/// Expands `source`; `collapse` puts an expansion on one line (macro bodies).
fn expand(
    source: &[char],
    registry: &MacroRegistry,
    diagnostics: &mut Vec<MacroDiagnostic>,
    source_name: &str,
    stack: &[String],
    collapse: bool,
) -> (Vec<char>, Vec<MacroExpansionSpan>) {
    if !collapse && !source.contains(&'#') {
        return (source.to_vec(), Vec::new());
    }
    let mut e = Expander {
        source,
        out: Vec::with_capacity(source.len()),
        index: 0,
        collapse,
        output_line: 1,
        output_column: 1,
        input_line: 1,
        input_column: 1,
    };
    let mut spans = Vec::new();
    let (mut in_line_comment, mut in_block_comment, mut in_string, mut in_string_expr, mut in_char) = (false, false, false, false, false);
    let len = source.len();

    while e.index < len {
        let current = source[e.index];
        let next = source.get(e.index + 1).copied();

        if in_line_comment {
            if current == '\n' {
                e.newline();
                in_line_comment = false;
                e.skip_to(e.index + 1);
            } else if !collapse {
                e.copy(1);
            } else {
                e.skip_to(e.index + 1);
            }
            continue;
        }
        if in_block_comment {
            if current == '*' && next == Some('/') {
                e.copy(2);
                in_block_comment = false;
            } else if current == '\n' {
                e.newline();
                e.skip_to(e.index + 1);
            } else {
                e.copy(1);
            }
            continue;
        }
        if in_string {
            if in_string_expr {
                in_string_expr = current != '>';
                e.copy(1);
            } else if current == '<' {
                in_string_expr = true;
                e.copy(1);
            } else if current == '\n' {
                e.newline();
                e.skip_to(e.index + 1);
            } else if current == '\\' && e.index + 1 < len {
                e.copy(2);
            } else {
                in_string = current != '"';
                e.copy(1);
            }
            continue;
        }
        if in_char {
            if current == '\n' {
                e.newline();
                e.skip_to(e.index + 1);
            } else if current == '\\' && e.index + 1 < len {
                e.copy(2);
            } else {
                in_char = current != '\'';
                e.copy(1);
            }
            continue;
        }

        if current == '/' && next == Some('/') {
            if collapse {
                e.skip_to(e.index + 2);
            } else {
                e.copy(2);
            }
            in_line_comment = true;
            continue;
        }
        if current == '/' && next == Some('*') {
            e.copy(2);
            in_block_comment = true;
            continue;
        }
        if current == '"' {
            in_string = true;
            e.copy(1);
            continue;
        }
        if current == '\'' {
            in_char = true;
            e.copy(1);
            continue;
        }
        if current == '\r' && next == Some('\n') {
            e.skip_to(e.index + 1);
            continue;
        }
        if current == '\n' {
            e.newline();
            e.skip_to(e.index + 1);
            continue;
        }
        if current != '#' {
            e.copy(1);
            continue;
        }

        // `#NAME` call site.
        let (call_site_line, call_site_column) = (e.input_line, e.input_column);
        let report_here = |diagnostics: &mut Vec<MacroDiagnostic>, message: String| {
            report(diagnostics, source_name, call_site_line, call_site_column, message)
        };
        let Some(name_units) = read_identifier(source, e.index + 1) else {
            e.copy(1);
            continue;
        };
        let macro_name = s(name_units);
        let after_name = e.index + name_units.len() + 1;
        let Some(m) = registry.macros.get(&macro_name) else {
            report_here(diagnostics, format!("cannot find macro `#{macro_name}`"));
            e.copy(after_name - e.index);
            continue;
        };
        if stack.contains(&macro_name) {
            let chain = stack.iter().chain([&macro_name]).cloned().collect::<Vec<_>>().join(" -> ");
            report_here(diagnostics, format!("cyclic macro expansion: {chain}"));
            e.copy(after_name - e.index);
            continue;
        }

        let scan = skip_inline_whitespace(source, after_name);
        let mut call_args = Vec::new();
        let mut call_end = after_name;
        let mut has_call_args = false;
        if scan < len && source[scan] == '(' {
            let Some((args, end)) = parse_call_arguments(source, scan) else {
                report_here(diagnostics, format!("macro call `#{macro_name}(` is missing its closing `)`"));
                e.copy((scan + 1).min(len) - e.index);
                continue;
            };
            has_call_args = true;
            call_args = args;
            call_end = end;
        }
        if !m.args.is_empty() && !has_call_args {
            report_here(diagnostics, format!("macro `#{macro_name}` takes {} argument(s), but none were supplied", m.args.len()));
            e.copy(call_end - e.index);
            continue;
        }
        if m.args.is_empty() && has_call_args {
            report_here(diagnostics, format!("macro `#{macro_name}` takes no arguments"));
            e.copy(call_end - e.index);
            continue;
        }
        let mut arg_values: Vec<Vec<char>> = call_args.iter().take(m.args.len()).cloned().collect();
        arg_values.resize(m.args.len(), Vec::new());
        if !m.args.is_empty() && call_args.len() != m.args.len() {
            report_here(diagnostics, format!("macro `#{macro_name}` takes {} argument(s), but {} were supplied", m.args.len(), call_args.len()));
        }
        let semicolon = skip_inline_whitespace(source, call_end);
        if semicolon < len && source[semicolon] == ';' {
            call_end = semicolon + 1;
        }

        let body = apply_args(&m.body, &m.args, &arg_values);
        let mut inner_stack = stack.to_vec();
        inner_stack.push(macro_name.clone());
        let (expanded, _) = expand(&body, registry, diagnostics, &m.file, &inner_stack, true);

        let (start_line, start_column) = (e.output_line, e.output_column);
        e.append(&expanded);
        let (end_line, end_column_exclusive) = (e.output_line, e.output_column);
        e.skip_to(call_end);
        spans.push(MacroExpansionSpan {
            start_line,
            start_column,
            end_line,
            end_column: start_column.max(end_column_exclusive - 1),
            call_site_line,
            call_site_column,
            call_end_line: e.input_line,
            call_end_column: e.input_column,
            end_column_exclusive,
            arg_values: arg_values.iter().map(|a| s(a)).collect(),
            macro_name,
            macro_file: m.file.clone(),
            macro_line: m.line,
        });
    }
    (e.out, spans)
}

/// `(a, (b, c), "d,e")` → trimmed top-level arguments
/// (comments dropped) and the index after `)`; `None` if unclosed.
fn parse_call_arguments(source: &[char], start: usize) -> Option<(Vec<Vec<char>>, usize)> {
    let len = source.len();
    let mut index = start + 1;
    let mut depth = 1;
    let mut current = Vec::new();
    let mut args = Vec::new();
    let (mut in_string, mut in_char) = (false, false);
    while index < len {
        let ch = source[index];
        let next = source.get(index + 1).copied();
        if in_string || in_char {
            current.push(ch);
            if ch == '\\' && let Some(n) = next {
                current.push(n);
                index += 2;
                continue;
            }
            if in_string && ch == '"' {
                in_string = false;
            } else if in_char && ch == '\'' {
                in_char = false;
            }
            index += 1;
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_string = ch == '"';
            in_char = ch == '\'';
            current.push(ch);
            index += 1;
            continue;
        }
        if ch == '(' {
            depth += 1;
            current.push(ch);
            index += 1;
            continue;
        }
        if ch == '/' && next == Some('/') {
            index += 2;
            while index < len && source[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if ch == '/' && next == Some('*') {
            index += 2;
            while index < len {
                if source[index] == '*' && source.get(index + 1) == Some(&'/') {
                    index += 2;
                    break;
                }
                index += 1;
            }
            continue;
        }
        if ch == ')' {
            depth -= 1;
            if depth == 0 {
                let trimmed = trim(&current);
                if !trimmed.is_empty() || !args.is_empty() {
                    args.push(trimmed.to_vec());
                }
                return Some((args, index + 1));
            }
            current.push(ch);
            index += 1;
            continue;
        }
        if ch == ',' && depth == 1 {
            args.push(trim(&current).to_vec());
            current.clear();
            index += 1;
            continue;
        }
        current.push(ch);
        index += 1;
    }
    None
}

/// Replaces whole identifiers equal to a parameter name, outside strings,
/// chars and comments. String interpolations (`"<…>"`) are copied raw too, so
/// arguments are not substituted there (deliberate).
fn apply_args(body: &[char], names: &[Vec<char>], values: &[Vec<char>]) -> Vec<char> {
    if names.is_empty() {
        return body.to_vec();
    }
    // A repeated name takes the later value.
    let mut args: HashMap<&[char], &[char]> = HashMap::new();
    for (i, n) in names.iter().enumerate() {
        args.insert(n, values.get(i).map_or(&[][..], Vec::as_slice));
    }
    let len = body.len();
    let mut out = Vec::with_capacity(len);
    let mut index = 0;
    let (mut in_line_comment, mut in_block_comment, mut in_string, mut in_string_expr, mut in_char) = (false, false, false, false, false);
    while index < len {
        let current = body[index];
        let next = body.get(index + 1).copied();
        if in_line_comment {
            out.push(current);
            in_line_comment = current != '\n';
            index += 1;
            continue;
        }
        if in_block_comment {
            if current == '*' && next == Some('/') {
                out.extend_from_slice(&['*', '/']);
                index += 2;
                in_block_comment = false;
            } else {
                out.push(current);
                index += 1;
            }
            continue;
        }
        if in_string {
            if in_string_expr {
                out.push(current);
                in_string_expr = current != '>';
                index += 1;
            } else if current == '<' {
                in_string_expr = true;
                out.push(current);
                index += 1;
            } else if current == '\\' && index + 1 < len {
                out.extend_from_slice(&body[index..index + 2]);
                index += 2;
            } else {
                out.push(current);
                in_string = current != '"';
                index += 1;
            }
            continue;
        }
        if in_char {
            if current == '\\' && index + 1 < len {
                out.extend_from_slice(&body[index..index + 2]);
                index += 2;
            } else {
                out.push(current);
                in_char = current != '\'';
                index += 1;
            }
            continue;
        }
        if current == '/' && (next == Some('/') || next == Some('*')) {
            in_line_comment = next == Some('/');
            in_block_comment = next == Some('*');
            out.extend_from_slice(&body[index..index + 2]);
            index += 2;
            continue;
        }
        if current == '"' || current == '\'' {
            in_string = current == '"';
            in_char = current == '\'';
            out.push(current);
            index += 1;
            continue;
        }
        if is_identifier_char(current) {
            let ident = read_identifier(body, index).expect("starts with an identifier char");
            out.extend_from_slice(args.get(ident).copied().unwrap_or(ident));
            index += ident.len();
            continue;
        }
        out.push(current);
        index += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_and_enddef_patterns() {
        assert_eq!(match_define(&u("  #define FOO_1+.:, a, b")).map(|(n, r)| (s(n), s(r))), Some(("FOO_1+.:".into(), " a, b".into())));
        assert!(match_define(&u("#defineFOO,")).is_none(), "needs whitespace");
        assert!(match_define(&u("#define FOO")).is_none(), "needs a comma");
        assert!(match_define(&u("#define FOO, a\rb")).is_none(), "the rest is one line");
        assert!(match_define(&u("#define FOO, a\u{2028}b")).is_some(), "only \\r and \\n end a line");
        assert!(match_define(&u("\u{85}#define FOO,")).is_some(), "leading whitespace is char::is_whitespace");
        assert!(match_enddef(&u("#enddef")) && match_enddef(&u(" #enddef+")) && !match_enddef(&u("#enddefx")));
        assert_eq!(s(strip_line_comment(&u("a \"//\" '/' b // c  "))), "a \"//\" '/' b");
    }

    #[test]
    fn expansion_collapses_and_maps_spans() {
        let mut reg = MacroRegistry::default();
        let mut diags = Vec::new();
        parse_macro_file("m.macro", "#define TWO, x\nmes(x); // c\nmes(\"x<x>\");\n#enddef\n#define TWO, y\n#enddef", &mut reg, &mut diags);
        assert_eq!(diags.len(), 1, "redefinition reported, first kept");
        let (text, spans) = expand_source_with_map("a\r\n#TWO(1) ;b", &reg, &mut diags, "f.rs2");
        assert_eq!(text, "a\nmes(1);  mes(\"x<x>\");b", "no substitution inside <…>");
        assert_eq!((spans[0].start_line, spans[0].start_column, spans[0].end_column), (2, 1, 21));
        assert_eq!(expand_source_with_map("a\r\nb", &reg, &mut diags, "f.rs2").0, "a\r\nb", "no '#': untouched");
    }
}
