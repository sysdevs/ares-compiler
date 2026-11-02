"""Write tests/typecheck/inputs.json: type-checking cases built from a fixed
prelude (commands, procs, labels, timers, queues) plus seeded random scripts
that mix valid and invalid expressions, statements, constants, dynamic
commands and feature flags.

usage (from ares-compiler/): python -I tools/make_typecheck_corpus.py
"""
import json
import random
from pathlib import Path

rng = random.Random(20261020)


def info(m, **extra):
    base = {k: {} for k in ["vartype", "protect", "require", "require2", "conditional", "set", "set2", "corrupt", "corrupt2"]}
    base.update(extra)
    return {"max": 100, "map": m, **base}


SYMBOLS = {
    "command": info({"0": "mes", "1": "npc_setvar", "2": "random", "3": "tostring", "4": "escape", "5": "seqtest"},
                    require={"1": "active_npc"}, require2={"1": "active_npc2"}),
    "runescript": info({"0": "[proc,two]"}),
    "constant": info({"max": "99", "str": "hello", "cyc1": "^cyc2", "cyc2": "^cyc1", "self": "^self", "npcref": "man",
                      "hex": "0x10", "nul": "null", "bad": "(", "calc3": "calc(1+2)", "boolc": "true", "coordc": "0_50_50_0_0",
                      "strnum": "5", "call": "random(5)", "neg": "-5", "big": "99999999999", "chr": "'c'", "nested": "^max",
                      "emoji": "'\U0001f600'", "lng": "9223372036854775807"}),
    "npc": info({"0": "man", "1": "goblin", "2": "Big Rat"}),
    "obj": info({"0": "coins", "1": "bones"}),
    "loc": info({"0": "door"}),
    "category": info({"0": "animals"}),
    "inv": info({"0": "bank"}),
    "enum": info({"0": "e1"}),
    "struct": info({"0": "st1"}),
    "dbrow": info({"0": "row1"}),
    "dbtable": info({"0": "tab1"}),
    "dbcolumn": info({"0": "col_int", "1": "col_pair", "2": "col_str"}, vartype={"0": "int", "1": "int,string", "2": "string"}),
    "param": info({"0": "p_int", "1": "p_str", "2": "p_obj"}, vartype={"0": "int", "1": "string", "2": "namedobj"}),
    "varp": info({"0": "vp_int", "1": "vp_str", "2": "vp_obj"}, vartype={"0": "int", "1": "string", "2": "namedobj"}),
    "varbit": info({"0": "vb"}, vartype={"0": "int"}),
    "varn": info({"0": "vn"}, vartype={"0": "int"}),
    "vars": info({"0": "vs"}, vartype={"0": "string"}),
}

PRELUDE = """[command,mes](string $text)
[command,npc_setvar](int $v)
[command,random](int $n)(int)
[command,tostring](int $n)(string)
[command,escape](string $s)(string)
[command,seqtest]
[command,pair](int $a)(int, string)
[command,coordx](coord $c)(int)
[command,npc_find](coord $c, npc $n, int $r)(boolean)
[command,noret]
[proc,two](int $a, int $b)(int, int)
[proc,one](int $a)(int)
[proc,none]
[proc,ret_str]()(string)
[label,lbl](int $a)
[label,lbl0]
[timer,tick](int $a)
[timer,tick0]
[queue,q1](int $a)
[queue,q2](int $a, string $b)
[queue,q0]
[softtimer,soft0]
[command,queue]
[command,queue*]
[command,longqueue]
[command,strongqueue]
[command,settimer]
[command,softtimer]
[command,enum]
[command,lc_param]
[command,nc_param]
[command,npc_param]
[command,struct_param]
[command,db_find]
[command,db_find_with_count]
[command,db_getfield]
[command,dump]
"""

INTS = ["0", "1", "-1", "7", "2147483647", "2147483648", "-2147483648", "-2147483649", "0x10", "0xFFFFFFFF", "0x100000000",
        "0b101", "99999999999999999999999999999999999999999", "007"]
NAMES = ["man", "goblin", "big_rat", "coins", "door", "animals", "bank", "e1", "st1", "row1", "col_int", "col_pair", "p_int",
         "p_str", "seqtest", "noret", "tostring", "nope", "int", "string", "npc", "namedobj", "two", "lbl", "tick", "q1",
         "true", "null", "default", "vp_int"]
LOCALS = ["$a", "$s", "$b", "$arr", "$nope", "$l", "$o", "$p"]


def expr(d=2):
    if d <= 0:
        return rng.choice(INTS + ['"txt"', '""', "'c'", "true", "false", "null", "0_50_50_1_1"] + NAMES + LOCALS
                          + ["$arr(0)", "%vp_int", "%vp_str", "%vb", ".%vp_int", "%nope", "^max", "^str", "^nope", "^cyc1",
                             "^self", "^npcref", "^hex", "^nul", "^bad", "^calc3", "^boolc", "^coordc", "^strnum", "^call",
                             "^neg", "^big", "^chr", "^nested", "^emoji", "^lng"])
    r = rng.randrange(22)
    sub = lambda: expr(d - 1)
    args = lambda n: ", ".join(sub() for _ in range(n))
    return [
        lambda: f"({sub()})",
        lambda: f"calc({arith(d - 1)})",
        lambda: f"mes({sub()})",
        lambda: f"random({sub()})",
        lambda: f"tostring({sub()})",
        lambda: f"pair({sub()})",
        lambda: f"~two({args(rng.randint(0, 3))})",
        lambda: f"~one({sub()})",
        lambda: rng.choice(["~none", "~ret_str", "~nope", "~one", "@lbl0", "@lbl(1)", "@nope"]),
        lambda: f"queue{rng.choice(['', '*'])}(q{rng.choice(['0', '1', '2', 'x'])}, {sub()}{rng.choice(['', ', 1'])})" + rng.choice(["", f"({args(rng.randint(0, 2))})"]),
        lambda: f"{rng.choice(['longqueue', 'weakqueue', 'strongqueue', '.queue'])}(q1, 0, {args(rng.randint(1, 2))})",
        lambda: f"settimer({rng.choice(['tick', 'tick0', 'soft0', 'nope'])}, {args(rng.randint(1, 2))})",
        lambda: f"softtimer(soft0, {sub()})",
        lambda: f"enum({rng.choice(['int', 'string', 'npc', 'nope', '1'])}, {rng.choice(['int', 'string', 'obj', 'nope'])}, {rng.choice(['e1', 'man', '1'])}, {sub()})",
        lambda: f"{rng.choice(['lc_param', 'nc_param', 'oc_param', 'struct_param'])}({rng.choice(['door', 'man', 'coins', 'st1', '1'])}, {rng.choice(['p_int', 'p_str', 'p_obj', 'man'])})",
        lambda: f"{rng.choice(['npc_param', 'loc_param', 'obj_param'])}({rng.choice(['p_int', 'p_str', 'man'])})",
        lambda: f"{rng.choice(['db_find', 'db_find_with_count'])}({rng.choice(['col_int', 'col_pair', 'col_str', 'man'])}, {sub()})",
        lambda: f"db_getfield({rng.choice(['row1', 'man'])}, {rng.choice(['col_int', 'col_pair', 'col_str', 'p_int'])}, {sub()})",
        lambda: f"dump({args(rng.randint(0, 2))})" if rng.random() < 0.8 else rng.choice(["dump", "script", "script(1)"]),
        lambda: f'"a<{sub()}>b<col=ff>c"',
        lambda: f"npc_find({sub()}, {sub()}, {sub()})",
        lambda: sub(),
    ][r]()


def arith(d):
    if d <= 0 or rng.random() < 0.35:
        return expr(d - 1)
    return f"{arith(d - 1)} {rng.choice(['+', '-', '*', '/', '%', '&', '|'])} {arith(d - 1)}" if rng.random() < 0.85 else f"({arith(d - 1)})"


def cond(d=2):
    if d <= 0 or rng.random() < 0.3:
        return expr(1)
    op = rng.choice(["<", ">", "<=", ">=", "=", "!", "&", "|"])
    return rng.choice([f"{cond(d - 1)} {op} {cond(d - 1)}", f"({cond(d - 1)})"])


def stmt(d=2):
    r = rng.randrange(16 if d > 0 else 10)
    return [
        lambda: f"def_{rng.choice(['int', 'string', 'boolean', 'long', 'coord', 'npc', 'obj', 'namedobj', 'nope', 'enum', 'dbrow', 'varp'])} ${rng.choice(['a', 's', 'b', 'l', 'o', 'p'])}{rng.choice(['', ' = ' + expr()])};",
        lambda: f"def_{rng.choice(['int', 'string', 'boolean', 'nope', 'coord'])} $arr({expr(1)});",
        lambda: f"{rng.choice(LOCALS + ['$arr(1)', '%vp_int', '%vp_str', '.%vp_int', '%nope'])} = {expr()};",
        lambda: f"$a, $s = {rng.choice(['~two(1, 2)', 'pair(1)', '1, 2', '1, \"x\"', expr()])};",
        lambda: f"$arr(0), $a = 1, 2;",
        lambda: f"{expr()};",
        lambda: f"return{rng.choice(['', '()', '(' + expr() + ')', '(1, 2)'])};",
        lambda: ";",
        lambda: f"mes({expr()});",
        lambda: f"{rng.choice(['seqtest', 'noret', 'tostring', 'nope', 'man', '1', '\"s\"', '~none', '@lbl0', 'calc(1+2)', 'dump(1)'])};",
        lambda: f"if ({cond()}) {stmt(d - 1)}" + (f" else {stmt(d - 1)}" if rng.random() < 0.4 else ""),
        lambda: f"while ({cond()}) {stmt(d - 1)}",
        lambda: "{ " + " ".join(stmt(d - 1) for _ in range(rng.randint(0, 3))) + " }",
        lambda: f"switch_{rng.choice(['int', 'npc', 'string', 'boolean', 'nope', 'obj'])} ({expr(1)}) {{ "
        + " ".join(f"case {rng.choice(['default', expr(0), expr(0) + ', ' + expr(0), '^max', 'calc(1)'])} : {stmt(d - 1)}" for _ in range(rng.randint(0, 3)))
        + " }",
        lambda: f"def_int $a = {expr()}; $a = calc($a {rng.choice(['+', '*'])} {expr(1)});",
        lambda: f"if ({cond()}) {{ def_int $x = 1; }} else {{ def_int $x = 2; }}",
    ][r]()


HEADS = ["[proc,t{i}]", "[proc,t{i}](int $a, string $s)", "[proc,t{i}](int $a)(int)", "[proc,t{i}]()(string)",
         "[label,t{i}](int $a)", "[opnpc1,man]", "[proc,t{i}](intarray $arr, int $a)", "[proc,t{i}](namedobj $o, npc $p)",
         "[proc,t{i}](long $l)(boolean)"]
FEATURES = [{}, {}, {}, {}, {"procs": False}, {"booleans": False}, {"calc": False}, {"logicalAnd": False},
            {"relationalEquals": False}, {"topLevelDefOnly": True}, {"enums": False, "structs": False, "dbtables": False},
            {"queueTyped": False}]

EDGE = [
    "[proc,e0] mes(^cyc1); mes(^self); def_int $a = ^bad; def_string $s = ^call; def_int $b = ^emoji; def_long $l = ^lng;",
    "[proc,e1] switch_nope (1) { case 1 : ; }",
    "[proc,e2] switch_nope (^max) { }",
    "[proc,e3] dump(@lbl0);",
    "[proc,e4] def_int $a = 2147483648; def_long $l = 9223372036854775808; def_int $h = 0xFFFFFFFF; def_boolean $b = 1; def_npc $n = 1;",
    "[proc,e5] def_string $s = man; def_string $t = seqtest; def_npc $n = \"goblin\"; def_obj $o = coins; def_string $u = true;",
    "[label,e6] @lbl(1); @lbl0; jump;",
    "[proc,e7] if (1) ; if (1 = 1 & 2) ; if ((1 = 1) = (2 = 2)) ; if (\"a\" = \"b\") ; if (~none = 1) ; if (pair(1) = 1) ;",
]


def main():
    inputs = [{"symbols": SYMBOLS, "features": {}, "files": [{"name": "aa_prelude.rs2", "text": PRELUDE}, {"name": "edge.rs2", "text": e}]} for e in EDGE]
    for n in range(500):
        scripts = []
        for i in range(rng.randint(1, 3)):
            head = rng.choice(HEADS).format(i=f"{n}_{i}")
            body = "\n".join(stmt() for _ in range(rng.randint(1, 5)))
            scripts.append(head + "\n" + body)
        inputs.append({"symbols": SYMBOLS, "features": rng.choice(FEATURES),
                       "files": [{"name": "aa_prelude.rs2", "text": PRELUDE}, {"name": "case.rs2", "text": "\n".join(scripts)}]})
    out = Path("tests/typecheck/inputs.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(inputs, ensure_ascii=False, indent=0) + "\n", encoding="utf-8")
    print(f"{len(inputs)} cases -> {out}")


main()
