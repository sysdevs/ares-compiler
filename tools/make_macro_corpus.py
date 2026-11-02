"""Write tests/macros/inputs.json: hand-picked macro preprocessor cases plus
seeded random macro files and sources.

usage (from ares-compiler/): python -I tools/make_macro_corpus.py
"""
import json
import random
from pathlib import Path

rng = random.Random(20261018)

DEFS = """#define NOARGS,
mes("noargs");
#enddef
#define ONE, x
mes(x); // trailing comment x
#enddef
#define TWO, a, b
if (a = b) { mes("a<a>b"); } /* block
 a */ 'a';
#enddef
#define NEST, v
#ONE(v)
#ONE("<v>")
#enddef
#define CYCLE_A,
#CYCLE_B
#enddef
#define CYCLE_B,
#CYCLE_A
#enddef
#define DUP, q
q
#enddef
#define DUP,
second
#enddef
  #define   SPACED+.:  ,  p1 ,, p2 // c, p3
p1+p2 p1.p2 p1:p2 p1_p2
#enddef
#define CRLF, z\r
z;\r
z;\r
#enddef\r
#define DOUBLE, x, x
x
#enddef
#enddef
#define SEP ,
bad
#enddef
#define UNCLOSED, y
y
"""

EDGE_SOURCES = [
    "no hash here\r\nkept\r\n",
    "#NOARGS\n#NOARGS;\n#NOARGS ;\n#NOARGS\n\n ;x",
    "#ONE(1)#ONE( 2 , 3 )\n#ONE()\n#ONE\n#ONE(\"a,b\")(x)",
    "#TWO(1, (2, 3))\n#TWO(\"x)\", ')')\n#TWO(a // c\n, b /* d */)",
    "#NEST(9)\n#CYCLE_A\n#UNDEFINED(1)\n#\n# x\n#1abc",
    "\"#ONE(1)\" '#' // #ONE(1)\n/* #ONE(1) */ #ONE(1)",
    "a\r\nb\r#ONE(1)\r\n\"s\r\n\" '\r'\r\n",
    "﻿#NOARGS\n\U0001f600#ONE(\U0001f600)\U0001f600\n#ONE(\"<x>\")",
    "#ONE(1",
    "#ONE(\"unterminated",
    "#SPACED+.:(A, B)\n#DUP\n#DUP(1)\n#DOUBLE(1, 2)\n#CRLF(9)",
    "#TWO(1)\n#TWO(1, 2, 3)\n#NOARGS(1)",
    "x #ONE(1) y #ONE(2)\n#ONE(3)",
    "\"<#ONE(1)>\" '\\'#ONE(1)' \"\\\"#ONE(2)\"",
]

PIECES = ["#NOARGS", "#ONE(1)", "#ONE( $x , y )", "#TWO(a, b)", "#TWO((1,2), \"x,y\")", "#NEST(q)", "#CYCLE_A",
          "#DUP", "#SPACED+.:(1,2)", "#CRLF(c)", "#MISSING", "#", "#ONE(", ";", " ", "\n", "\r\n", "\t",
          "\"s<$x>#ONE(1)\"", "'#'", "// #ONE(1)\n", "/* #ONE\n(1) */", "mes(1);", "\U0001f600", "x", " "]


def random_defs():
    out = []
    for i in range(rng.randint(1, 4)):
        name = rng.choice(["R", "S", "ONE", "NOARGS", f"M{i}"])
        args = rng.choice(["", "a", "a, b", " a ,b,", "a,a"])
        body = rng.choice(["a", "a b", "mes(\"a<a>\");", "#ONE(a)", "#R", "// a\nb", "'a' a", "/* a */ a\nb"])
        out.append(f"#define {name}, {args}\n{body}\n#enddef")
    return rng.choice(["\n", "\r\n"]).join(out) + rng.choice(["", "\n", "\n#enddef"])


def main():
    inputs = [{"macros": [{"file": "aa_defs.macro", "text": DEFS}], "sources": [{"name": f"e{i}.rs2", "text": t} for i, t in enumerate(EDGE_SOURCES)]}]
    inputs.append({"macros": [{"file": "a.macro", "text": "#define X,\nfrom_a\n#enddef"}, {"file": "b.macro", "text": "#define X,\nfrom_b\n#enddef"}],
                   "sources": [{"name": "x.rs2", "text": "#X"}]})
    for _ in range(400):
        macros = [{"file": "aa_defs.macro", "text": DEFS}] + [{"file": f"r{j}.macro", "text": random_defs()} for j in range(rng.randint(0, 2))]
        sources = [{"name": f"s{j}.rs2", "text": "".join(rng.choice(PIECES) for _ in range(rng.randint(1, 14)))} for j in range(3)]
        inputs.append({"macros": macros, "sources": sources})
    out = Path("tests/macros/inputs.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(inputs, ensure_ascii=False, indent=0) + "\n", encoding="utf-8")
    print(f"{len(inputs)} cases -> {out}")


main()
