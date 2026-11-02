"""Write tests/parser/inputs.json: hand-picked parser edge cases plus seeded
random scripts/expressions generated from RuneScriptParser.g4, and token-level
mutations of them (to probe accept/reject).

usage (from ares-compiler/): python -I tools/make_parser_corpus.py
"""
import json
import random
from pathlib import Path

rng = random.Random(20261017)

EDGE_FILES = [
    "",
    "[proc,a]",
    "[proc,a]()",
    "[proc,a]()()",
    "[proc,a](int $x)(int)",
    "[proc,a](int $x)(int) ;",
    "[proc,a](int $x)(int);",
    "[proc,a](int $x)(int) (1);",
    "[proc,a] (1);",
    "[proc,a b c] return;",
    "[proc,a*]",
    "[opheld1,_] ;",
    "[proc,a](intarray $arr, string $s)(int, string) return(1, \"x\");",
    "[proc,a] if ($x = 1) if ($y = 2) mes(\"a\"); else mes(\"b\");",
    "[proc,a] switch_int ($x) { case 1, 2 : mes(\"a\"); case default : ; }",
    "[proc,a] switch_int ($x) { case default, 1 : ; }",
    "[proc,a] switch_int ($x) { }",
    "[proc,a] switch_int($x);",
    "[proc,a] def_int $x = 1; def_int $y; def_int $z(5); def_int;",
    "[proc,a] $a, %b, .%c, $d(1) = 1, 2, 3, 4;",
    "[proc,a] $x;",
    "[proc,a] $x(1);",
    "[proc,a] default; calc(1); default(1);",
    "[proc,a] return(calc(a * (b) + (c)));",
    "[proc,a] return(calc(a*(b)(c)));",
    "[proc,a] return(calc(%a % %b));",
    "[proc,a] return(calc(1 -2));",
    "[proc,a] return(calc(1 - 2 * 3 & 4 | 5 / 6 % 7));",
    "[proc,a] if (1 < 2 = 3 < 4 & 5 ! 6 | 7 >= 8) ;",
    "[proc,a] if ((1)) ; if (((1 = 2))) ; if ((1) = (2)) ;",
    "[proc,a] mes(\"\"); mes(\"text\"); mes(\"<col=ff>a</col><br><$x>b<p,x><col=\");",
    "[proc,a] mes(\"a\\\"b\\<c\\\\\");",
    "[proc,a] mes('a'); mes('\\''); mes('\\\\');",
    "[proc,a] mes('\U0001f600');",
    "[proc,a] mes(\"\U0001f600<$x>\U0001f600\");",
    "[proc,a] x = 0_50_50_10_10; y(0_50_50_10_10, 4_63_63_63_63, 99999999999_1_1_1_1);",
    "[proc,a] y(0x1F, 0b101, -5, 2147483648, true, false, null);",
    "[proc,a] ~p; ~p(); ~p(1, 2); @l; @l(1);",
    "[proc,a] cmd*(1)(2); cmd*()(); .cmd(1);",
    "[proc,a] $if = 1; $1 = 2; ^calc; %while;",
    "[proc,a] while (1 = 1) { }",
    "[proc,a] return;\n[proc,b] return;",
    "[proc,a] // c\n /* d */ return;",
    "[proc,a] mes(\"bad\\x\");",
    "[proc,a] mes(\"unterminated);",
    "[proc,a] # ;",
    "[proc,a] mes(\"<\"<\"x\">\">\");",
]
EDGE_EXPRS = ["1", "-1", "\"a\"", "\"a<$x>\"", "^c", "$x(1)", "calc(1+2)", "1 = 2", "", "a b", "'ab'", "'\U0001f600'"]

NAMES = ["a", "x", "mes", "default", "def_int", "intarray", "0x10", "true", "null", "0_1_2", "switch_int", "a.b", "c:d"]
ADV = NAMES + ["1", "if", "while", "calc", "return", "else"]


def ident():
    return rng.choice(NAMES)


def string(d):
    parts = []
    for _ in range(rng.randint(0, 3)):
        r = rng.random()
        if r < 0.4:
            parts.append(rng.choice(["text", " ", "a\\\"b", "\\<", "\\\\", "\U0001f600"]))
        elif r < 0.6:
            parts.append(rng.choice(["<col=ff0000>", "</col>", "<br>", "<col=", "<p,abc>", "<u>", "<img=1>"]))
        else:
            parts.append("<" + expr(d - 1) + ">")
    return '"' + "".join(parts) + '"'


def args(d, lo=0):
    return "(" + ", ".join(expr(d - 1) for _ in range(rng.randint(lo, 2))) + ")"


def expr(d=3):
    if d <= 0:
        return rng.choice(["1", "-2", "0x1f", "0b11", "true", "null", "'c'", '"s"', "$v", "%g", "^k", ident()])
    r = rng.randrange(17)
    return [
        lambda: "(" + expr(d - 1) + ")",
        lambda: "calc(" + arith(d - 1) + ")",
        lambda: ident() + args(d),
        lambda: ident() + "*" + args(d) + args(d),
        lambda: "~" + ident() + (args(d) if rng.random() < 0.6 else ""),
        lambda: "@" + ident() + (args(d) if rng.random() < 0.6 else ""),
        lambda: "$" + rng.choice(ADV),
        lambda: "$" + rng.choice(ADV) + "(" + expr(d - 1) + ")",
        lambda: rng.choice(["%", ".%"]) + rng.choice(ADV),
        lambda: "^" + rng.choice(ADV),
        lambda: rng.choice(["0", "-7", "2147483647", "0xFF", "0b1", "0_50_50_1_2", "true", "false", "null", "'x'", "'\\''"]),
        lambda: string(d),
        lambda: string(d),
        lambda: ident(),
        lambda: ident(),
        lambda: expr(0),
        lambda: expr(d - 1),
    ][r]()


def arith(d):
    if d <= 0 or rng.random() < 0.3:
        return expr(d - 1) if rng.random() < 0.8 else "(" + arith(d - 1) + ")"
    return arith(d - 1) + " " + rng.choice(["*", "/", "%", "+", "-", "&", "|"]) + " " + arith(d - 1)


def cond(d):
    if d <= 0 or rng.random() < 0.3:
        return expr(d - 1) if rng.random() < 0.8 else "(" + cond(d - 1) + ")"
    return cond(d - 1) + " " + rng.choice(["<", ">", "<=", ">=", "=", "!", "&", "|"]) + " " + cond(d - 1)


def stmt(d=3):
    r = rng.randrange(12 if d > 0 else 6)
    var = lambda: rng.choice(["$" + rng.choice(ADV), "$x(" + expr(1) + ")", "%g", ".%g"])
    return [
        lambda: expr(d) + ";",
        lambda: ";",
        lambda: "def_" + rng.choice(["int", "string", "coord"]) + " $" + rng.choice(ADV) + rng.choice(["", " = " + expr(d)]) + ";",
        lambda: "def_int $" + rng.choice(ADV) + "(" + expr(d) + ");",
        lambda: ", ".join(var() for _ in range(rng.randint(1, 3))) + " = " + ", ".join(expr(d) for _ in range(rng.randint(1, 3))) + ";",
        lambda: "return" + rng.choice(["", "()", args(d, 1)]) + ";",
        lambda: "{ " + " ".join(stmt(d - 1) for _ in range(rng.randint(0, 3))) + " }",
        lambda: "if (" + cond(d) + ") " + stmt(d - 1) + (" else " + stmt(d - 1) if rng.random() < 0.5 else ""),
        lambda: "while (" + cond(d) + ") " + stmt(d - 1),
        lambda: "switch_int (" + expr(d) + ") { "
        + " ".join(
            "case " + rng.choice(["default", ", ".join(expr(1) for _ in range(rng.randint(1, 2)))]) + " : " + " ".join(stmt(d - 1) for _ in range(rng.randint(0, 2)))
            for _ in range(rng.randint(0, 3))
        )
        + " }",
        lambda: "if (" + cond(d) + ") if (" + cond(1) + ") " + stmt(0) + " else " + stmt(0),
        lambda: expr(d) + ";",
    ][r]()


def script():
    head = "[" + rng.choice(["proc", "label", "opheld1", "clientscript"]) + "," + " ".join(ident() for _ in range(rng.randint(1, 2))) + rng.choice(["", "*"]) + "]"
    r = rng.random()
    if r < 0.3:
        params = "(" + ", ".join(rng.choice(["int", "intarray", "string"]) + " $" + rng.choice(ADV) for _ in range(rng.randint(0, 2))) + ")"
        if rng.random() < 0.5:
            params += "(" + ", ".join(rng.choice(["int", "string"]) for _ in range(rng.randint(0, 2))) + ")"
        head += params
    return head + "\n" + "\n".join(stmt() for _ in range(rng.randint(0, 4)))


def lex(text):
    """Rough whitespace split good enough to mutate at token granularity."""
    return text.replace("(", " ( ").replace(")", " ) ").replace(";", " ; ").replace(",", " , ").split()


def mutate(text):
    toks = lex(text)
    if not toks:
        return text
    i = rng.randrange(len(toks))
    op = rng.randrange(3)
    if op == 0:
        del toks[i]
    elif op == 1:
        toks.insert(i, toks[rng.randrange(len(toks))])
    else:
        j = rng.randrange(len(toks))
        toks[i], toks[j] = toks[j], toks[i]
    return " ".join(toks)


def main():
    inputs = [{"entry": "file", "text": t} for t in EDGE_FILES]
    inputs += [{"entry": "expr", "text": t} for t in EDGE_EXPRS]
    for _ in range(2000):
        text = "\n".join(script() for _ in range(rng.randint(1, 2)))
        inputs.append({"entry": "file", "text": text})
        if rng.random() < 0.6:
            inputs.append({"entry": "file", "text": mutate(text)})
    for _ in range(600):
        e = expr(3)
        inputs.append({"entry": "expr", "text": e})
        inputs.append({"entry": "expr", "text": mutate(e)})
    out = Path("tests/parser/inputs.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(inputs, ensure_ascii=False, indent=0) + "\n", encoding="utf-8")
    print(f"{len(inputs)} inputs -> {out}")


main()
