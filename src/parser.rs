//! The RuneScript parser and AST builder, on chumsky.
//!
//! Only accept/reject and the tree are pinned: the tree is discarded on any
//! syntax error. Alternatives are tried in order with backtracking, ordered so
//! that a locally successful alternative is the intended one:
//! - ambiguous inputs go to the lowest-numbered grammar alternative (a text-only
//!   string is a `StringLiteral`, not a `JoinedStringExpression`; `case default:`
//!   is the default case; `(x)` in a condition/calc is the parenthesised
//!   condition/arithmetic form, which builds the same node);
//! - a longer alternative that shares a prefix with a shorter one is tried first
//!   (`$a(i)` before `$a`, `name*(..)(..)` before `name(..)`, assignment before
//!   expression statement, array declaration after plain declaration fails);
//! - optional groups are greedy (dangling `else`, script parameter/return groups).
//!
//! Node locations come from the matched token range:
//! start = first token, end = last token's column + its UTF-16 length (min 1).
//! The parser sees only default-channel tokens plus a trailing `EOF`, so a
//! `ScriptFile` ends at `<EOF>`.

use chumsky::pratt::{infix, left};
use chumsky::prelude::*;

use crate::ast::expr::*;
use crate::ast::stmt::{Statement, SwitchCase};
use crate::ast::{Loc, Parameter, Script, ScriptFile, Token as AstToken};
use crate::lexer::{tokenize, Lexed, Token, TokenKind as K};
use crate::FatalError;

type In<'a> = &'a [K];
type Ex<'a> = extra::Err<Rich<'a, K>>;
type P<'a, O> = Boxed<'a, 'a, In<'a>, O, Ex<'a>>;

/// A syntax error (1-based column, offsets applied).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SyntaxError {
    pub line: i32,
    pub column: i32,
    pub message: String,
}

/// `node` is `None` when the parser reported any error; lexer errors are reported
/// but do not discard the tree.
#[derive(Clone, PartialEq, Debug)]
pub struct Parsed<T> {
    pub node: Option<T>,
    pub errors: Vec<SyntaxError>,
}

/// Parses a whole (macro-expanded) source file.
pub fn parse_script_file(src: &str, name: &str) -> Result<Parsed<ScriptFile>, FatalError> {
    parse(src, name, 0, 0, |g| g.script_file)
}

/// Parses a single expression (constant values); `line_offset` applies to every line, `column_offset` to line 1 only.
pub fn parse_single_expression(src: &str, name: &str, line_offset: i32, column_offset: i32) -> Result<Parsed<Expression>, FatalError> {
    parse(src, name, line_offset, column_offset, |g| g.single_expression)
}

struct Cx<'a> {
    toks: Vec<Token<'a>>,
    name: &'a str,
    line_offset: i32,
    column_offset: i32,
}

impl Cx<'_> {
    /// Location spanning tokens `start..=stop`.
    fn range(&self, start: usize, stop: usize) -> Loc {
        let (a, b) = (&self.toks[start], &self.toks[stop]);
        let col_off = |line: u32| if line == 1 { self.column_offset } else { 0 };
        let len = b.text.chars().count().max(1) as i32;
        Loc {
            name: self.name.to_owned(),
            line: a.line as i32 + self.line_offset,
            column: a.column as i32 + 1 + col_off(a.line),
            end_line: b.line as i32 + self.line_offset,
            end_column: b.column as i32 + 1 + col_off(b.line) + len,
        }
    }

    fn loc(&self, span: SimpleSpan) -> Loc {
        self.range(span.start, span.end - 1)
    }

    fn text(&self, i: usize) -> &str {
        self.toks[i].text
    }

    fn token(&self, i: usize) -> AstToken {
        AstToken { source: self.range(i, i), text: self.text(i).to_owned() }
    }

    fn ident(&self, i: usize) -> Identifier {
        Identifier { source: self.range(i, i), ty: None, type_hint: None, text: self.text(i).to_owned(), reference: None }
    }

    fn syntax_error(&self, line: u32, column: u32, message: String) -> SyntaxError {
        let col_off = if line == 1 { self.column_offset } else { 0 };
        SyntaxError { line: line as i32 + self.line_offset, column: column as i32 + col_off + 1, message }
    }
}

struct Grammar<'a> {
    script_file: P<'a, ScriptFile>,
    single_expression: P<'a, Expression>,
}

fn parse<T>(
    src: &str,
    name: &str,
    line_offset: i32,
    column_offset: i32,
    entry: impl for<'a> Fn(Grammar<'a>) -> P<'a, T>,
) -> Result<Parsed<T>, FatalError> {
    let Lexed { tokens, errors: lex_errors, eof_line, eof_column } = tokenize(src);
    let mut toks: Vec<Token> = tokens.into_iter().filter(|t| t.channel == 0).collect();
    toks.push(Token { kind: K::EOF, text: "<EOF>", line: eof_line, column: eof_column, channel: 0, start: src.len() });
    let cx = Cx { toks, name, line_offset, column_offset };
    let kinds: Vec<K> = cx.toks.iter().map(|t| t.kind).collect();

    let mut errors: Vec<SyntaxError> = lex_errors
        .iter()
        .map(|e| {
            // An unterminated string is reported at its opening quote.
            let quote = cx.toks.iter().rev().find(|t| t.kind == K::QUOTE_OPEN && (t.line, t.column) < (e.line, e.column));
            match quote.filter(|_| e.text.ends_with('\n')) {
                Some(q) => cx.syntax_error(q.line, q.column, e.message()),
                None => cx.syntax_error(e.line, e.column, e.message()),
            }
        })
        .collect();
    let result = entry(grammar(&cx)).parse(&kinds).into_result();
    let node = match result {
        Ok(node) => {
            check_char_literals(&cx)?;
            Some(node)
        }
        Err(errs) => {
            // ponytail: one diagnostic at the first failure, no error recovery. Diagnostics are compared loosely (presence per file).
            let at = errs.first().map_or(cx.toks.len() - 1, |e| e.span().start.min(cx.toks.len() - 1));
            let t = &cx.toks[at];
            // A failure after a lexer error (an unterminated string) is that error's fallout.
            if lex_errors.iter().all(|e| (t.line, t.column) < (e.line, e.column)) {
                errors.push(match unclosed(&cx.toks).filter(|_| t.kind == K::EOF) {
                    Some(open) => cx.syntax_error(open.line, open.column, format!("unclosed `{}`: the file ends before it is closed", open.text)),
                    None => cx.syntax_error(t.line, t.column, expected_message(errs.first(), t)),
                });
            }
            None
        }
    };
    Ok(Parsed { node, errors })
}

/// `expected one of `;`, `)`, found `x``: the first few expected tokens by name.
fn expected_message(e: Option<&Rich<K>>, found: &Token) -> String {
    let mut names: Vec<String> = e
        .into_iter()
        .flat_map(|e| e.expected())
        .filter_map(|p| match p {
            chumsky::error::RichPattern::Token(k) => Some(token_name(**k)),
            chumsky::error::RichPattern::Label(l) => Some(l.to_string()),
            chumsky::error::RichPattern::EndOfInput => Some("end of file".to_owned()),
            _ => None,
        })
        .collect();
    names.sort();
    names.dedup();
    let found = if found.kind == K::EOF { "end of file".to_owned() } else { format!("`{}`", found.text) };
    const SHOWN: usize = 6;
    match names.len() {
        0 => format!("unexpected {found}"),
        1 => format!("expected {}, found {found}", names[0]),
        n if n <= SHOWN => format!("expected one of {}, found {found}", names.join(", ")),
        n => format!("expected one of {} and {} others, found {found}", names[..SHOWN].join(", "), n - SHOWN),
    }
}

/// The innermost `(`, `[` or `{` still open at the end of `toks`.
fn unclosed<'t>(toks: &'t [Token<'t>]) -> Option<&'t Token<'t>> {
    let mut open = Vec::new();
    for t in toks {
        match t.kind {
            K::LPAREN | K::LBRACK | K::LBRACE => open.push(t),
            K::RPAREN | K::RBRACK | K::RBRACE => {
                open.pop();
            }
            _ => {}
        }
    }
    open.pop()
}

/// How a token kind reads in an `expected …` message.
fn token_name(k: K) -> String {
    let fixed = match k {
        K::LPAREN => "(",
        K::RPAREN => ")",
        K::COLON => ":",
        K::SEMICOLON => ";",
        K::COMMA => ",",
        K::LBRACK => "[",
        K::RBRACK => "]",
        K::LBRACE => "{",
        K::RBRACE => "}",
        K::PLUS => "+",
        K::MINUS => "-",
        K::MUL => "*",
        K::DIV => "/",
        K::DOTMOD => ".%",
        K::MOD => "%",
        K::AND => "&",
        K::OR => "|",
        K::EQ => "=",
        K::EXCL => "!",
        K::DOLLAR => "$",
        K::CARET => "^",
        K::TILDE => "~",
        K::AT => "@",
        K::GT => ">",
        K::GTE => ">=",
        K::LT => "<",
        K::LTE => "<=",
        K::IF => "if",
        K::ELSE => "else",
        K::WHILE => "while",
        K::CASE => "case",
        K::DEFAULT => "default",
        K::RETURN => "return",
        K::CALC => "calc",
        K::NULL_LITERAL => "null",
        K::QUOTE_OPEN | K::QUOTE_CLOSE => "\"",
        K::STRING_EXPR_START => "<",
        K::STRING_EXPR_END => ">",
        _ => {
            return match k {
                K::TYPE_ARRAY => "array type",
                K::DEF_TYPE => "`def_<type>`",
                K::SWITCH_TYPE => "`switch_<type>`",
                K::INTEGER_LITERAL | K::HEX_LITERAL | K::BIN_LITERAL => "number",
                K::COORD_LITERAL => "coord",
                K::MAPZONE_LITERAL => "mapzone",
                K::BOOLEAN_LITERAL => "`true` or `false`",
                K::CHAR_LITERAL => "character",
                K::IDENTIFIER => "identifier",
                K::EOF => "end of file",
                _ => "string text",
            }
            .to_owned();
        }
    };
    format!("`{fixed}`")
}

/// A character literal must unescape to exactly one char; the first one in
/// source order that does not is a fatal error.
fn check_char_literals(cx: &Cx) -> Result<(), FatalError> {
    for t in cx.toks.iter().filter(|t| t.kind == K::CHAR_LITERAL) {
        let cleaned = unescape(&t.text[1..t.text.len() - 1]);
        if cleaned.chars().count() != 1 {
            return Err(FatalError(format!("invalid character literal: text={}, cleaned={cleaned}", t.text)));
        }
    }
    Ok(())
}

/// Drops each escaping backslash; the lexer only admits `\\ \' \" \<`, so it cannot throw here.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            out.push(chars.next().unwrap_or('\\'));
        } else {
            out.push(c);
        }
    }
    out
}

/// `level_mx_mz_lx_lz` packed into an i32. The lexer guarantees five digit
/// groups; an oversized group wraps (only its masked low bits are kept).
fn pack_coord(text: &str) -> i32 {
    let p: Vec<i32> = text.split('_').map(|s| s.bytes().fold(0i32, |a, b| a.wrapping_mul(10).wrapping_add((b - b'0') as i32))).collect();
    let x = p[1].wrapping_shl(6) | (p[3] & 0x3fff);
    let z = p[2].wrapping_shl(6) | (p[4] & 0x3fff);
    let y = p[0] & 0x3;
    z | x.wrapping_shl(14) | y.wrapping_shl(28)
}

const IDENTIFIER_KINDS: [K; 10] = [
    K::IDENTIFIER,
    K::HEX_LITERAL,
    K::BOOLEAN_LITERAL,
    K::NULL_LITERAL,
    K::COORD_LITERAL,
    K::MAPZONE_LITERAL,
    K::TYPE_ARRAY,
    K::SWITCH_TYPE,
    K::DEF_TYPE,
    K::DEFAULT,
];
const ADVANCED_IDENTIFIER_KINDS: [K; 16] = [
    K::IDENTIFIER,
    K::HEX_LITERAL,
    K::BOOLEAN_LITERAL,
    K::NULL_LITERAL,
    K::COORD_LITERAL,
    K::MAPZONE_LITERAL,
    K::TYPE_ARRAY,
    K::SWITCH_TYPE,
    K::DEF_TYPE,
    K::DEFAULT,
    K::INTEGER_LITERAL,
    K::IF,
    K::ELSE,
    K::WHILE,
    K::RETURN,
    K::CALC,
];

/// Index of a single matched token of one of `kinds`.
fn tok<'a>(kinds: &'static [K]) -> impl Parser<'a, In<'a>, usize, Ex<'a>> + Clone {
    one_of(kinds).map_with(|_, e| {
        let span: SimpleSpan = e.span();
        span.start
    })
}

fn kw<'a>(k: K) -> impl Parser<'a, In<'a>, (), Ex<'a>> + Clone {
    just(k).ignored()
}

fn grammar<'a>(cx: &'a Cx<'a>) -> Grammar<'a> {
    let paren = |p: P<'a, Expression>| p.delimited_by(kw(K::LPAREN), kw(K::RPAREN));

    let identifier = tok(&IDENTIFIER_KINDS).map(|i| cx.ident(i)).boxed();
    let advanced_identifier = tok(&ADVANCED_IDENTIFIER_KINDS).map(|i| cx.ident(i)).boxed();

    let mut expression = Recursive::<chumsky::recursive::Indirect<In<'a>, Expression, Ex<'a>>>::declare();
    let expr: P<'a, Expression> = expression.clone().boxed();

    let expression_list = expr.clone().separated_by(kw(K::COMMA)).at_least(1).collect::<Vec<_>>().boxed();
    // `( expressionList? )` → the list or empty.
    let args = expression_list
        .clone()
        .or_not()
        .map(Option::unwrap_or_default)
        .delimited_by(kw(K::LPAREN), kw(K::RPAREN))
        .boxed();
    // `parenthesis` yields its inner expression.
    let parenthesis = paren(expr.clone());

    let wrapped = move |expression: Expression, span: SimpleSpan| WrappedExpression {
        source: cx.loc(span),
        ty: None,
        type_hint: None,
        expression: Box::new(expression),
    };
    let binary = move |make: fn(BinaryExpression) -> Expression| {
        move |l: Expression, op: usize, r: Expression, e: &mut chumsky::input::MapExtra<'a, '_, In<'a>, Ex<'a>>| {
            make(BinaryExpression {
                source: cx.loc(e.span()),
                ty: None,
                type_hint: None,
                left: Box::new(l),
                operator: cx.token(op),
                right: Box::new(r),
            })
        }
    };

    let mut arithmetic = Recursive::<chumsky::recursive::Indirect<In<'a>, Expression, Ex<'a>>>::declare();
    arithmetic.define(
        paren(arithmetic.clone().boxed())
            .map_with(move |x, e| Expression::Parenthesized(wrapped(x, e.span())))
            .or(expr.clone())
            .pratt((
                infix(left(4), tok(&[K::MUL, K::DIV, K::MOD]), binary(Expression::Arithmetic)),
                infix(left(3), tok(&[K::PLUS, K::MINUS]), binary(Expression::Arithmetic)),
                infix(left(2), tok(&[K::AND]), binary(Expression::Arithmetic)),
                infix(left(1), tok(&[K::OR]), binary(Expression::Arithmetic)),
            )),
    );

    let mut condition = Recursive::<chumsky::recursive::Indirect<In<'a>, Expression, Ex<'a>>>::declare();
    condition.define(
        paren(condition.clone().boxed())
            .map_with(move |x, e| Expression::Parenthesized(wrapped(x, e.span())))
            .or(expr.clone())
            .pratt((
                infix(left(4), tok(&[K::LT, K::GT, K::LTE, K::GTE]), binary(Expression::Condition)),
                infix(left(3), tok(&[K::EQ, K::EXCL]), binary(Expression::Condition)),
                infix(left(2), tok(&[K::AND]), binary(Expression::Condition)),
                infix(left(1), tok(&[K::OR]), binary(Expression::Condition)),
            )),
    );
    let condition = condition.boxed();

    let calc = kw(K::CALC)
        .ignore_then(paren(arithmetic.boxed()))
        .map_with(move |x, e| Expression::Calc(wrapped(x, e.span())));

    // Deliberate: only the groups that are present are collected, so in
    // `name*()(a)` the second group lands in `arguments`.
    let star_group = expression_list.clone().or_not().delimited_by(kw(K::LPAREN), kw(K::RPAREN)).boxed();
    let command_call = identifier
        .clone()
        .then_ignore(kw(K::MUL))
        .then(star_group.clone())
        .then(star_group)
        .map(|((name, a), b)| {
            let mut present = a.into_iter().chain(b);
            (name, present.next().unwrap_or_default(), Some(present.next().unwrap_or_default()))
        })
        .or(identifier.clone().then(args.clone()).map(|(name, a)| (name, a, None)))
        .map_with(move |(name, arguments, arguments2), e| {
            Expression::CommandCall(CommandCallExpression { source: cx.loc(e.span()), ty: None, type_hint: None, name, arguments, arguments2, symbol: None })
        });
    let (call_identifier, call_args) = (identifier.clone(), args.clone());
    let call = move |sigil: K, make: fn(CallExpression) -> Expression| {
        kw(sigil).ignore_then(call_identifier.clone()).then(call_args.clone().or_not()).map_with(move |(name, arguments), e| {
            make(CallExpression { source: cx.loc(e.span()), ty: None, type_hint: None, name, arguments: arguments.unwrap_or_default(), symbol: None })
        })
    };

    let local_variable = kw(K::DOLLAR)
        .ignore_then(advanced_identifier.clone())
        .then(parenthesis.clone().or_not())
        .map_with(move |(name, index), e| {
            Expression::LocalVariable(LocalVariableExpression {
                source: cx.loc(e.span()),
                ty: None,
                type_hint: None,
                name,
                index: index.map(Box::new),
                reference: None,
            })
        })
        .boxed();
    let game_variable = just(K::MOD)
        .to(false)
        .or(just(K::DOTMOD).to(true))
        .then(advanced_identifier.clone())
        .map_with(move |(dot, name), e| {
            Expression::GameVariable(GameVariableExpression { source: cx.loc(e.span()), ty: None, type_hint: None, dot, name, reference: None })
        })
        .boxed();
    let constant_variable = kw(K::CARET).ignore_then(advanced_identifier.clone()).map_with(move |name, e| {
        Expression::ConstantVariable(ConstantVariableExpression {
            source: cx.loc(e.span()),
            ty: None,
            type_hint: None,
            name,
            sub_expression: None,
            reference: None,
        })
    });

    let literal = move |kinds: &'static [K], f: fn(&str) -> LiteralValue| {
        tok(kinds).map(move |i| {
            let source = cx.range(i, i);
            let (ty, type_hint) = (None, None);
            match f(cx.text(i)) {
                LiteralValue::Integer(value, radix) => {
                    Expression::IntegerLiteral(IntegerLiteral { source, ty, type_hint, value, radix, reference: None, number_value: None })
                }
                LiteralValue::Coord(value) => Expression::CoordLiteral(Literal { source, ty, type_hint, value, reference: None }),
                LiteralValue::Boolean(value) => Expression::BooleanLiteral(Literal { source, ty, type_hint, value, reference: None }),
                LiteralValue::Char(value) => Expression::CharacterLiteral(Literal { source, ty, type_hint, value, reference: None }),
                LiteralValue::Null => Expression::NullLiteral(Literal { source, ty, type_hint, value: -1, reference: None }),
            }
        })
    };
    let string_literal = kw(K::QUOTE_OPEN)
        .ignore_then(tok(&[K::STRING_TEXT]).repeated().collect::<Vec<_>>())
        .then_ignore(kw(K::QUOTE_CLOSE))
        .map_with(move |parts, e| {
            let raw: String = parts.iter().map(|&i| cx.text(i)).collect();
            Expression::StringLiteral(StringLiteral {
                source: cx.loc(e.span()),
                ty: None,
                type_hint: None,
                value: unescape(&raw),
                sub_expression: None,
                reference: None,
            })
        });
    let string_part = choice((
        tok(&[K::STRING_TEXT]).map(move |i| StringPart::Basic { source: cx.range(i, i), value: unescape(cx.text(i)) }),
        tok(&[K::STRING_TAG, K::STRING_CLOSE_TAG, K::STRING_PARTIAL_TAG])
            .map(move |i| StringPart::Basic { source: cx.range(i, i), value: cx.text(i).to_owned() }),
        tok(&[K::STRING_P_TAG]).map(move |i| StringPart::PTag { source: cx.range(i, i), value: cx.text(i).to_owned() }),
        expr.clone()
            .delimited_by(kw(K::STRING_EXPR_START), kw(K::STRING_EXPR_END))
            .map_with(move |expression, e| StringPart::Expression { source: cx.loc(e.span()), expression }),
    ));
    let joined_string = kw(K::QUOTE_OPEN)
        .ignore_then(string_part.repeated().collect::<Vec<_>>())
        .then_ignore(kw(K::QUOTE_CLOSE))
        .map_with(move |parts, e| {
            Expression::JoinedString(JoinedStringExpression { source: cx.loc(e.span()), ty: None, type_hint: None, parts })
        });

    expression.define(choice((
        parenthesis.clone().map_with(move |x, e| Expression::Parenthesized(wrapped(x, e.span()))),
        calc,
        command_call,
        call(K::TILDE, Expression::ProcCall),
        call(K::AT, Expression::JumpCall),
        local_variable.clone(),
        game_variable.clone(),
        constant_variable,
        choice((
            literal(&[K::INTEGER_LITERAL], |t| LiteralValue::Integer(t.to_owned(), 10)),
            literal(&[K::HEX_LITERAL], |t| LiteralValue::Integer(t[2..].to_owned(), 16)),
            literal(&[K::BIN_LITERAL], |t| LiteralValue::Integer(t[2..].to_owned(), 2)),
            literal(&[K::COORD_LITERAL], |t| LiteralValue::Coord(pack_coord(t))),
            literal(&[K::BOOLEAN_LITERAL], |t| LiteralValue::Boolean(t == "true")),
            // Validity (one UTF-16 unit) is checked after a successful parse.
            literal(&[K::CHAR_LITERAL], |t| LiteralValue::Char(unescape(&t[1..t.len() - 1]).chars().next().unwrap_or('\0'))),
            literal(&[K::NULL_LITERAL], |_| LiteralValue::Null),
            string_literal,
        )),
        joined_string,
        identifier.clone().map(Expression::Identifier),
    )).labelled("expression"));

    let mut statement = Recursive::<chumsky::recursive::Indirect<In<'a>, Statement, Ex<'a>>>::declare();
    let stmt = statement.clone().boxed();
    let statements = stmt.clone().repeated().collect::<Vec<_>>().boxed();

    let block = statements
        .clone()
        .delimited_by(kw(K::LBRACE), kw(K::RBRACE))
        .map_with(move |statements, e| Statement::Block { source: cx.loc(e.span()), statements });
    let return_ = kw(K::RETURN)
        .ignore_then(args.clone().or_not())
        .then_ignore(kw(K::SEMICOLON))
        .map_with(move |a, e| Statement::Return { source: cx.loc(e.span()), expressions: a.unwrap_or_default() });
    let paren_condition = paren(condition);
    let if_ = kw(K::IF)
        .ignore_then(paren_condition.clone())
        .then(stmt.clone())
        .then(kw(K::ELSE).ignore_then(stmt.clone()).or_not())
        .map_with(move |((condition, then_statement), else_statement), e| Statement::If {
            source: cx.loc(e.span()),
            condition,
            then_statement: Box::new(then_statement),
            else_statement: else_statement.map(Box::new),
        });
    let while_ = kw(K::WHILE).ignore_then(paren_condition).then(stmt.clone()).map_with(move |(condition, then_statement), e| {
        Statement::While { source: cx.loc(e.span()), condition, then_statement: Box::new(then_statement) }
    });
    let switch_case = kw(K::CASE)
        .ignore_then(kw(K::DEFAULT).then(kw(K::COLON)).to(Vec::new()).or(expression_list.clone().then_ignore(kw(K::COLON))))
        .then(statements.clone())
        .map_with(move |(keys, statements), e| SwitchCase { source: cx.loc(e.span()), keys, statements });
    let switch = tok(&[K::SWITCH_TYPE])
        .then(parenthesis.clone())
        .then(switch_case.repeated().collect::<Vec<_>>().delimited_by(kw(K::LBRACE), kw(K::RBRACE)))
        .map_with(move |((t, condition), cases), e| Statement::Switch {
            source: cx.loc(e.span()),
            type_token: cx.token(t),
            condition,
            cases,
            ty: None,
            default_case: None,
        });
    let def_head = tok(&[K::DEF_TYPE]).then_ignore(kw(K::DOLLAR)).then(advanced_identifier.clone()).boxed();
    let declaration = def_head
        .clone()
        .then(kw(K::EQ).ignore_then(expr.clone()).or_not())
        .then_ignore(kw(K::SEMICOLON))
        .map_with(move |((t, name), initializer), e| Statement::Declaration {
            source: cx.loc(e.span()),
            type_token: cx.token(t),
            name,
            initializer,
            symbol: None,
        });
    let array_declaration = def_head
        .then(parenthesis.clone())
        .then_ignore(kw(K::SEMICOLON))
        .map_with(move |((t, name), initializer), e| Statement::ArrayDeclaration {
            source: cx.loc(e.span()),
            type_token: cx.token(t),
            name,
            initializer,
            symbol: None,
        });
    let assignment = local_variable
        .or(game_variable)
        .separated_by(kw(K::COMMA))
        .at_least(1)
        .collect::<Vec<_>>()
        .then_ignore(kw(K::EQ))
        .then(expression_list.clone())
        .then_ignore(kw(K::SEMICOLON))
        .map_with(move |(vars, expressions), e| Statement::Assignment { source: cx.loc(e.span()), vars, expressions });
    let expression_statement = expr
        .clone()
        .then_ignore(kw(K::SEMICOLON))
        .map_with(move |expression, e| Statement::Expression { source: cx.loc(e.span()), expression });
    let empty = kw(K::SEMICOLON).map_with(move |_, e| Statement::Empty { source: cx.loc(e.span()) });

    statement.define(choice((
        block,
        return_,
        if_,
        while_,
        switch,
        declaration,
        array_declaration,
        assignment,
        expression_statement,
        empty,
    )).labelled("statement"));

    let script_name = identifier.clone().repeated().at_least(1).collect::<Vec<_>>().map_with(move |mut ids, e| {
        if ids.len() == 1 {
            ids.pop().unwrap()
        } else {
            let text = ids.iter().map(|i| i.text.as_str()).collect::<Vec<_>>().join(" ");
            Identifier { source: cx.loc(e.span()), ty: None, type_hint: None, text, reference: None }
        }
    });
    let parameter = tok(&[K::IDENTIFIER, K::TYPE_ARRAY])
        .then_ignore(kw(K::DOLLAR))
        .then(advanced_identifier.clone())
        .map_with(move |(t, name), e| Parameter { source: cx.loc(e.span()), type_token: cx.token(t), name, symbol: None });
    // `(parameterList?)` and `(typeList?)`: an empty group is the same as none.
    let parameters = parameter
        .separated_by(kw(K::COMMA))
        .at_least(1)
        .collect::<Vec<_>>()
        .or_not()
        .delimited_by(kw(K::LPAREN), kw(K::RPAREN));
    let returns = tok(&[K::IDENTIFIER])
        .map(move |i| cx.token(i))
        .separated_by(kw(K::COMMA))
        .at_least(1)
        .collect::<Vec<_>>()
        .or_not()
        .delimited_by(kw(K::LPAREN), kw(K::RPAREN));
    let script = kw(K::LBRACK)
        .ignore_then(identifier.clone())
        .then_ignore(kw(K::COMMA))
        .then(script_name)
        .then(kw(K::MUL).or_not())
        .then_ignore(kw(K::RBRACK))
        .then(parameters.then(returns.or_not()).or_not())
        .then(statements)
        .map_with(move |((((trigger, name), star), groups), statements), e| {
            let (parameters, return_tokens) = match groups {
                Some((p, r)) => (p, r.flatten()),
                None => (None, None),
            };
            Script {
                source: cx.loc(e.span()),
                trigger,
                name,
                is_star: star.is_some(),
                parameters,
                return_tokens,
                statements,
                trigger_type: None,
                parameter_type: None,
                return_type: None,
                symbol: None,
                subject_reference: None,
                block: None,
            }
        });
    let script_file = script
        .repeated()
        .collect::<Vec<_>>()
        .then_ignore(kw(K::EOF))
        .map_with(move |scripts, e| ScriptFile { source: cx.loc(e.span()), scripts })
        .boxed();

    let single_expression = expr.clone().then_ignore(kw(K::EOF)).boxed();
    Grammar { script_file, single_expression }
}

enum LiteralValue {
    Integer(String, u32),
    Coord(i32),
    Boolean(bool),
    Char(char),
    Null,
}
