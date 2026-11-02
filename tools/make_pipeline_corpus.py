"""Write tests/pipeline/inputs.json: mostly type-correct programs (a typed
expression generator over a fixed prelude) that reach code generation and
beyond, plus hand-picked edge programs. Feeds the codegen, pointer-checking
and writer gates.

usage (from ares-compiler/): python -I tools/make_pipeline_corpus.py
"""
import json
import random
from pathlib import Path

rng = random.Random(20261021)


def info(m, **extra):
    base = {k: {} for k in ["vartype", "protect", "require", "require2", "conditional", "set", "set2", "corrupt", "corrupt2"]}
    base.update(extra)
    return {"max": 100, "map": m, **base}


COMMANDS = ["mes", "random", "tostring", "escape", "npc_find", "p_delay", "npc_setvar", "pair", "coordx", "noret", "npc_del",
            "queue", "queue*", "longqueue", "weakqueue", "settimer", "softtimer", "enum", "nc_param", "npc_param", "lc_param",
            "db_find", "db_find_with_count", "db_getfield", "dump", "longpush", "strlen", "seqtest", "npc_findnext", "toString", "script"]
SCRIPTS = ["[proc,two]", "[proc,one]", "[proc,none]", "[proc,ret_str]", "[label,lbl]", "[label,lbl0]", "[timer,tick]",
           "[queue,q1]", "[queue,q2]", "[softtimer,soft0]", "[proc,lng]"]
SYMBOLS = {
    "command": info({str(i): n for i, n in enumerate(COMMANDS)},
                    require={"5": "active_player", "6": "active_npc", "20": "active_npc", "10": "active_npc"},
                    require2={"6": "active_npc2", "10": "active_npc2"},
                    set={"4": "active_npc", "28": "active_npc"}, conditional={"4": True, "28": True},
                    corrupt={"5": "active_npc"}),
    "runescript": info({str(i): n for i, n in enumerate(SCRIPTS)}),
    "constant": info({"max": "99", "str": "hello", "npcref": "man", "boolc": "true", "lng": "5", "coordc": "0_50_50_0_0"}),
    "npc": info({"0": "man", "1": "goblin"}),
    "obj": info({"0": "coins"}),
    "loc": info({"0": "door"}),
    "category": info({"0": "animals"}),
    "enum": info({"0": "e1"}),
    "dbrow": info({"0": "row1"}),
    "dbcolumn": info({"0": "col_int", "1": "col_str"}, vartype={"0": "int", "1": "string"}),
    "param": info({"0": "p_int", "1": "p_str"}, vartype={"0": "int", "1": "string"}),
    "varp": info({"0": "vp_int", "1": "vp_str", "2": "vp_long"}, vartype={"0": "int", "1": "string", "2": "long"}),
    "varbit": info({"0": "vb"}, vartype={"0": "int"}),
}

# Command ids start above the core opcodes (0..=100), like real ones.
SYMBOLS["command"] = {k: ({str(int(i) + 1000): x for i, x in v.items()} if isinstance(v, dict) else v) for k, v in SYMBOLS["command"].items()}

PRELUDE = """[command,mes](string $text)
[command,random](int $n)(int)
[command,tostring](int $n)(string)
[command,escape](string $s)(string)
[command,npc_find](coord $c, npc $n, int $r)(boolean)
[command,p_delay](int $n)
[command,npc_setvar](int $v)
[command,pair](int $a)(int, string)
[command,coordx](coord $c)(int)
[command,noret]
[command,npc_del]
[command,queue]
[command,queue*]
[command,longqueue]
[command,weakqueue]
[command,settimer]
[command,softtimer]
[command,enum]
[command,nc_param]
[command,npc_param]
[command,lc_param]
[command,db_find]
[command,db_find_with_count]
[command,db_getfield]
[command,dump]
[command,longpush](long $l)(long)
[command,strlen](string $s)(int)
[command,npc_findnext]()(boolean)
[command,toString](int $n)(string)
[command,script]
[proc,two](int $a, int $b)(int, int)
return($b, $a);
[proc,one](int $a)(int)
return(calc($a + 1));
[proc,none]
[proc,ret_str]()(string)
return("s");
[label,lbl](int $a)
mes(tostring($a));
[label,lbl0]
[timer,tick](int $a, string $s)
[queue,q1](int $a)
[queue,q2](int $a, string $b)
[softtimer,soft0]
"""


class Gen:
    def __init__(self, locals_):
        self.locals = locals_  # name -> type

    def local(self, t):
        names = [n for n, lt in self.locals.items() if lt == t]
        return f"${rng.choice(names)}" if names else None

    def expr(self, t, d=2):
        opts = []
        loc = self.local(t)
        if t == "int":
            opts = [lambda: rng.choice(["0", "1", "-7", "42", "2147483647", "-2147483648", "0x10", "0xFFFFFFFF", "0b101", "^max", "%vp_int", "%vb", "null", "'c'"][:13] if False else ["0", "1", "-7", "42", "2147483647", "-2147483648", "0x10", "0xFFFFFFFF", "0b101", "^max", "%vp_int", "%vb", "null"]),
                    lambda: f"calc({self.arith('int', d - 1)})", lambda: f"random({self.expr('int', d - 1)})",
                    lambda: f"~one({self.expr('int', d - 1)})", lambda: f"coordx({self.expr('coord', d - 1)})",
                    lambda: f"strlen({self.expr('string', d - 1)})", lambda: f"nc_param(man, p_int)", lambda: f"npc_param(p_int)",
                    lambda: f"enum(int, int, e1, {self.expr('int', d - 1)})", lambda: f"db_getfield(row1, col_int, 0)",
                    lambda: f"db_find_with_count(col_int, {self.expr('int', d - 1)})", lambda: f"(calc({self.expr('int', d - 1)}))"]
        elif t == "string":
            opts = [lambda: rng.choice(['"txt"', '""', '"a\\\\b"', "^str", "%vp_str", "null", "man", "1"]),
                    lambda: f"tostring({self.expr('int', d - 1)})", lambda: f"escape({self.expr('string', d - 1)})",
                    lambda: f'"a<{self.expr("int", d - 1)}>b<col=ff>c<{self.expr("string", d - 1)}>"', lambda: f'"x<{self.expr("string", d - 1)}>"',
                    lambda: "~ret_str", lambda: f"dump({', '.join(self.dumpable(d - 1) for _ in range(rng.randint(1, 3)))})",
                    lambda: "script", lambda: f"nc_param(man, p_str)", lambda: f"enum(int, string, e1, {self.expr('int', d - 1)})",
                    lambda: f"db_getfield(row1, col_str, 0)"]
        elif t == "boolean":
            opts = [lambda: rng.choice(["true", "false", "1", "0", "^boolc"]), lambda: "npc_findnext",
                    lambda: f"npc_find({self.expr('coord', d - 1)}, {self.expr('npc', d - 1)}, {self.expr('int', d - 1)})"]
        elif t == "long":
            opts = [lambda: rng.choice(["5", "-1", "9223372036854775807", "0xFFFFFFFFFFFFFFFF", "^lng", "%vp_long", "null"]),
                    lambda: f"calc({self.arith('long', d - 1)})", lambda: f"longpush({self.expr('long', d - 1)})",
                    lambda: f"~lng({self.expr('long', d - 1)})"]
        elif t == "coord":
            opts = [lambda: rng.choice(["0_50_50_0_0", "3_63_63_63_63", "^coordc", "null"])]
        elif t == "npc":
            opts = [lambda: rng.choice(["man", "goblin", '"goblin"', "^npcref", "null"])]
        elif t == "obj":
            opts = [lambda: rng.choice(["coins", "null"])]
        if loc and rng.random() < 0.35:
            return loc
        return rng.choice(opts)() if d > 0 else opts[0]()

    def dumpable(self, d):
        local = self.local("int") or self.local("string")
        return rng.choice([lambda: rng.choice(["man", "true", "'c'", "0_1_2_3_4", "null", "%vp_int", '"q<1>"', "tostring(1)", '"s"'] + ([local] if local else [])),
                           lambda: f"random({self.expr('int', d)})", lambda: f"escape({self.expr('string', d)})"])()

    def arith(self, t, d):
        if d <= 0 or rng.random() < 0.4:
            return self.expr(t, max(d, 0))
        return f"{self.arith(t, d - 1)} {rng.choice(['+', '-', '*', '/', '%', '&', '|'])} {self.arith(t, d - 1)}"

    def cond(self, d=2):
        if d > 0 and rng.random() < 0.35:
            return f"{rng.choice(['(', ''])}{self.cond(d - 1)} {rng.choice(['&', '|'])} {self.cond(d - 1)}{''}".replace("(", "(", 1) if False else f"({self.cond(d - 1)}) {rng.choice(['&', '|'])} {self.cond(d - 1)}"
        t = rng.choice(["int", "int", "boolean", "npc", "coord", "string"])
        if t in ("int", "long"):
            op = rng.choice(["=", "!", "<", ">", "<=", ">="])
        elif t == "string":
            return f"{self.expr('string', 1)} = \"x\"" if False else f"{self.expr('int', 1)} ! {self.expr('int', 1)}"
        else:
            op = rng.choice(["=", "!"])
        left = self.local(t) or {"int": "random(3)", "long": "longpush(5)", "boolean": "npc_findnext", "npc": "nc_param(man, p_int)" if False else None,
                                 "coord": None}.get(t)
        if left is None:
            t, op, left = "int", rng.choice(["=", "!", "<", ">="]), "random(3)"
        return f"{left} {op} {self.expr(t, 1)}"

    def stmt(self, ret, d=2):
        saved = dict(self.locals)
        out = self.stmt_inner(ret, d)
        if d < 2:
            self.locals = saved
        return out

    def stmt_inner(self, ret, d):
        r = rng.randrange(14 if d > 0 else 8)
        if r == 0:
            t = rng.choice(["int", "string", "boolean", "coord", "npc", "obj"])
            name = f"v{len(self.locals)}"
            s = f"def_{t} ${name}" + (f" = {self.expr(t)}" if rng.random() < 0.7 else "") + ";"
            self.locals[name] = t
            return s
        if r == 1:
            name = f"arr{len(self.locals)}"
            self.locals[name] = "intarray"
            return f"def_int ${name}({self.expr('int', 1)}); ${name}({self.expr('int', 1)}) = {self.expr('int', 1)}; mes(tostring(${name}(0)));"
        if r == 2:
            ints = [n for n, t in self.locals.items() if t == "int"]
            if len(ints) >= 2:
                return f"${ints[0]}, ${ints[1]} = ~two({self.expr('int', 1)}, {self.expr('int', 1)});"
            return f"%vp_int, .%vb = 1, 2;"
        if r == 3:
            return f"mes({self.expr('string')});"
        if r == 4:
            return rng.choice(["noret;", "npc_del;", "p_delay(1);", ".npc_setvar(1);", "npc_setvar(2);", "~none;", "random(1);", "pair(1);", "~two(1, 2);", "tostring(1);"])
        if r == 5:
            return rng.choice([f"queue(q1, 0, {self.expr('int', 1)});", f"queue*(q2, 1)({self.expr('int', 1)}, {self.expr('string', 1)});",
                               "queue*(q1, 0)(5);", f"longqueue(q1, 0, 1, 0);", "weakqueue(q1, 0, 3);",
                               f"settimer(tick, 5, {self.expr('int', 1)}, {self.expr('string', 1)});", "softtimer(soft0, 2);",
                               f"db_find(col_int, {self.expr('int', 1)});", f"db_find(col_str, {self.expr('string', 1)});"])
        if r == 6:
            if ret is None:
                return "return;"
            return f"return({', '.join(self.expr(t, 1) for t in ret)});"
        if r == 7:
            return f"{self.expr('string')};" if rng.random() < 0.2 else ";"
        if r == 8:
            return f"if ({self.cond()}) {{ {self.stmt(ret, d - 1)} }}" + (f" else {self.stmt(ret, d - 1)}" if rng.random() < 0.5 else "")
        if r == 9:
            return f"while ({self.cond()}) {{ {self.stmt(ret, d - 1)} }}"
        if r == 10:
            t = rng.choice(["int", "npc", "boolean", "coord"])
            keys = {"int": ["1", "2, 3", "^max", "0x10", "null"], "npc": ["man", "goblin", "^npcref", "\"goblin\""],
                    "boolean": ["true", "false"], "coord": ["0_50_50_0_0", "^coordc"]}[t]
            cases = rng.sample(keys, k=min(len(keys), rng.randint(0, 2)))
            body = " ".join(f"case {k} : {self.stmt(ret, d - 1)}" for k in cases)
            if rng.random() < 0.6:
                body = (f"case default : {self.stmt(ret, d - 1)} " + body) if rng.random() < 0.5 else (body + f" case default : {self.stmt(ret, d - 1)}")
            return f"switch_{t} ({self.expr(t, 1)}) {{ {body} }}"
        if r == 11:
            return "{ " + " ".join(self.stmt(ret, d - 1) for _ in range(rng.randint(0, 3))) + " }"
        if r == 12:
            return f"@lbl({self.expr('int', 1)});" if self.label else f"mes(dump({self.dumpable(1)}));"
        return f"mes(dump({self.dumpable(1)}, {self.dumpable(1)}));"


def script(i):
    params = rng.choice([[], [("int", "a")], [("int", "a"), ("string", "s")], [("npc", "n")], [("intarray", "xs")], [("coord", "c"), ("boolean", "b")]])
    rets = rng.choice([None, None, ["int"], ["string"], ["int", "string"], ["boolean"], ["npc"], ["coord"]])
    trigger = rng.choice(["proc", "proc", "proc", "label", "opnpc1", "softtimer"])
    if trigger != "proc":
        rets = None
    if trigger in ("opnpc1", "softtimer"):
        params = []
    name = {"opnpc1": rng.choice(["man", "_animals", "_"]), "softtimer": f"st{i}"}.get(trigger, f"g{i}")
    head = f"[{trigger},{name}]"
    if params or rets:
        head += "(" + ", ".join(f"{t} ${n}" for t, n in params) + ")"
        if rets:
            head += "(" + ", ".join(rets) + ")"
    g = Gen({n: t for t, n in params})
    g.label = trigger == "label"
    body = " ".join(g.stmt(rets) for _ in range(rng.randint(1, 6)))
    return head + "\n" + body


EDGE = [
    "[proc,e_dump] mes(dump(1, \"s\", man, true, 'c', null, 0_50_50_0_0, ^max, %vp_int, calc(1 + 2), random(1), tostring(1), \"a<1>b\"));",
    "[proc,e_dump2] mes(dump($nope));",
    "[proc,e_dump3] mes(dump(~one(1)));",
    "[proc,e_dump4](int $a) mes(dump(($a)));",
    "[proc,e_dump5] mes(dump(1 = 1));",
    "[proc,e_script] mes(script); mes(\"<script>\");",
    "[proc,e_long](long $l)(long) if ($l > 5) { return(calc($l * 2)); } return($l);",
    "[proc,e_long2] def_int $a = 1; longpush(5);",
    "[proc,e_debug](int $a, string $s) [debugproc,dbg](int $a, string $s) mes($s);",
    "[debugproc,dbg2](int $a, string $s, boolean $b) mes($s);",
    "[proc,e_str](string $s) if ($s = \"a\") { mes($s); }",
    "[proc,e_null] def_string $s = null; def_long $l = null; def_npc $n = null; def_int $i = null; def_coord $c;",
    "[proc,e_ret](int $a)(int, string, long, npc, boolean, coord)",
    "[proc,e_sw](int $a) switch_int ($a) { case default : mes(\"d\"); case 1 : mes(\"1\"); }",
    "[proc,e_sw2](npc $n) switch_npc ($n) { case man, goblin : ; case null : ; }",
    "[proc,e_cond](int $a) if (($a = 1 | $a = 2) & ($a ! 3 | ($a > 4 & $a < 9))) { mes(\"x\"); }",
    "[proc,e_strlit] mes(1); mes(true); mes(man); def_string $s = 5; def_string $t = false;",
]


def main():
    base = {"symbols": SYMBOLS, "features": {}}
    inputs = [dict(base, files=[{"name": "aa_prelude.rs2", "text": PRELUDE}, {"name": "edge.rs2", "text": e}]) for e in EDGE]
    for n in range(400):
        text = "\n".join(script(f"{n}_{i}") for i in range(rng.randint(1, 3)))
        inputs.append(dict(base, files=[{"name": "aa_prelude.rs2", "text": PRELUDE}, {"name": "case.rs2", "text": text}]))
    out = Path("tests/pipeline/inputs.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(inputs, ensure_ascii=False, indent=0) + "\n", encoding="utf-8")
    print(f"{len(inputs)} cases -> {out}")


main()
