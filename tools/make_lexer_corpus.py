"""Write tests/lexer/inputs.json: hand-picked lexer edge cases plus seeded random
strings over an alphabet weighted toward the grammar's interesting characters.

usage (from ares-compiler/): python -I tools/make_lexer_corpus.py
"""
import json
import random
from pathlib import Path

EDGE = [
    # rule-order ties against IDENTIFIER
    "if iff calc calcx true truex null nullx default",
    "intarray array xarray def_int def_intarray switch_int switch_ def_ switch_intarray",
    "123 123abc -123 -123abc 0x1F 0x1Fg 0X 0b101 0b102 0b 0x",
    "0_50_50_0_0 0_50_50 0_50_50_0 1_2_3_4_5_6 1__2_3 0_50_50_0_0x",
    "+ +5 a+1 : :a . .% a.% .%a a.b",
    "> >= < <= - -- -x / // /* */ /*/ /**/ /* a */ b */",
    "x // comment\ny", "x // no newline", "/* unterminated", "/* a\n b */ c",
    # char literals and their error paths
    "'a' '\\\\' '\\'' '' 'ab' '\\x' '\\\\x' '\n' 'a", "'", "'\\", "'\\\\", "x'", "'é' '😀'",
    # strings, tags, interpolation, depth
    '"plain"', '"a<x>b"', '"<col=ff0000>red</col>"', '"<br><u><img=0><gt><lt><shad=1><str>"',
    '"<col=<$x>>"', '"<p,abc>" "<p,>"', '"a\\"b\\<c\\\\d"', '"bad\\x"', '"line\nnext"',
    '"<"x">"', '"<a>=b>"', '"<a>b>c"', '">"', '"<$x + 1>"', '"<"<"y">">"', '"<col=>"',
    '"unterminated', '"a\\', "\"<x\"", "\"<col=a\nb>\"",
    # whitespace, CRLF, odd chars, non-ASCII columns
    "a\r\nb\r\n\tc", "#define X 1", "a ? b ` c \\ d", "é a", "😀 a", " a\u000cb", "a b",
    "[proc,x](int $a)(int)\nreturn(calc($a * 2));",
]

ALPHABET = list("abcdefilnrstuxy_019+-.:%/*<>=!\"'\\ \n\r\t()[]{},;$^~@&|#é") + [
    "if", "array", "def_", "switch_", "true", "null", "0x", "0b", "_", "//", "/*", "*/",
    "<col=", "<br>", "</col>", "<p,", "\\\"", "\\<", "\\\\", "😀",
]

# Token-level pieces (mostly valid), glued with or without separators so
# adjacent pieces also merge into longer identifiers and literals.
PIECES = [
    "if", "else", "while", "case", "default", "return", "calc", "true", "false", "null",
    "int", "intarray", "def_int", "def_coord", "switch_int", "switch_obj", "$x", "%v", ".%v", "^c", "~p", "@l",
    "0", "7", "-3", "42", "0x1f", "0XAB", "0b10", "0B2", "0_50_50", "0_50_50_1_1", "1_2", "3_4_5_6",
    "'a'", "'\\\\'", "'\\''", "'z", "+", "-", "*", "/", "%", ":", ".", "_", "=", "!", "&", "|",
    "<", ">", "<=", ">=", "(", ")", "[", "]", "{", "}", ",", ";",
    '"', '"s"', '"<$x>"', '"<col=ff>t</col>"', '"<br>"', '"<p,x>"', '"<col="', "<", ">",
    "// c\n", "/* c */", "/*", "*/", " ", "\n", "\t", "\r\n",
]

def main():
    rng = random.Random(20261016)
    inputs = list(EDGE)
    for _ in range(3000):
        n = rng.randint(1, 24)
        inputs.append("".join(rng.choice(ALPHABET) for _ in range(n)))
    for _ in range(3000):
        n = rng.randint(1, 12)
        inputs.append("".join(rng.choice(PIECES) + rng.choice(["", "", " "]) for _ in range(n)))
    out = Path("tests/lexer/inputs.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(inputs, ensure_ascii=False, indent=0) + "\n", encoding="utf-8")
    print(f"{len(inputs)} inputs -> {out}")

main()
