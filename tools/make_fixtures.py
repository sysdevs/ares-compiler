"""Generate the differential-test fixture corpus under tests/fixtures/.

Idempotent: re-running overwrites every generated file. Edit the CASES table to
add a case; keep file names ASCII-lowercase so their order is the same
bytewise and case-insensitively on every OS.
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "tests" / "fixtures"

# Every CompilerTypeInfo must carry ALL fields: deserialisation requires each
# table (`vartype`, `require`, ...).
EMPTY = {"vartype": {}, "protect": {}, "require": {}, "require2": {}, "conditional": {}, "set": {}, "set2": {}, "corrupt": {}, "corrupt2": {}}


def info(max_, map_, **over):
    d = {"max": max_, "map": map_, **json.loads(json.dumps(EMPTY))}
    d.update(over)
    return d


SYMBOLS = {
    "command": info(
        20,
        {"0": "mes", "1": "tostring", "2": "escape", "3": "random", "4": "npc_find", "5": "p_delay", "6": "npc_setvar", "7": "inv_total", "8": "coordx"},
        require={"5": "active_player", "6": "active_npc"},
        require2={"6": "active_npc2"},
        conditional={"4": True},
        set={"4": "active_npc"},
    ),
    "runescript": info(
        100,
        {
            "0": "[proc,literals]", "1": "[proc,strings]", "2": "[proc,control]", "3": "[proc,arith]", "4": "[proc,vars]",
            "5": "[proc,minmax]", "6": "[proc,swap]", "7": "[label,done]", "8": "[proc,caller]", "9": "[proc,commands]",
            "10": "[proc,macros]", "11": "[opnpc1,man]", "12": "[opnpc1,_]", "13": "[debugproc,dbg]", "14": "[proc,errors]",
            "15": "[proc,arrays]", "16": "[proc,tuples]", "17": "[label,loop]", "18": "[proc,crlf_kept]", "19": "[proc,broken]", "20": "[label,start]",
        },
    ),
    "constant": info(0, {"max_level": "99", "greeting": "hello", "spawn": "0_50_50_20_20"}),
    "npc": info(3, {"0": "man", "1": "woman", "2": "hans"}),
    "obj": info(2, {"0": "coins", "1": "bronze_sword"}),
    "inv": info(1, {"0": "inv"}),
    "category": info(1, {"0": "humans"}),
    "varp": info(3, {"0": "playtime", "1": "name_var", "2": "secret"}, vartype={"0": "int", "1": "string", "2": "int"}, protect={"0": False, "1": False, "2": True}),
    "varn": info(1, {"0": "npc_hp"}, vartype={"0": "int"}),
    "varo": info(1, {"0": "obj_charges"}, vartype={"0": "int"}),
    "vars": info(1, {"0": "world_counter"}, vartype={"0": "int"}),
    "varbit": info(1, {"0": "flag_bit"}, vartype={"0": "int"}, protect={"0": False}),
}

# Command ids start above the core opcodes (0..=100), like real ones.
SYMBOLS["command"] = {k: ({str(int(i) + 1000): x for i, x in v.items()} if isinstance(v, dict) else v) for k, v in SYMBOLS["command"].items()}

# Declared in every case so each case is self-contained.
COMMANDS = """[command,mes](string $text)
[command,tostring](int $value)(string)
[command,escape](string $text)(string)
[command,random](int $max)(int)
[command,npc_find](coord $coord, npc $npc)(boolean)
[command,p_delay](int $ticks)
[command,npc_setvar](int $value)
[command,inv_total](inv $inv, obj $obj)(int)
[command,coordx](coord $c)(int)
[command,dump]
"""

CASES: dict[str, dict[str, str | bytes]] = {
    "01-literals": {
        "literals.rs2": """[proc,literals]
def_int $a = 1;
def_int $b = -42;
def_int $c = 0x7fffffff;
def_int $d = 0xffffffff;
def_int $e = 0b1010;
def_int $f = 2147483647;
def_boolean $g = true;
def_boolean $h = false;
def_coord $i = 0_50_50_20_20;
def_coord $j = 3_12_7_63_63;
def_char $k = 'x';
def_char $l = '\\'';
def_string $m = "plain";
def_string $n = "";
def_npc $o = null;
def_string $p = null;
def_int $q = ^max_level;
def_string $r = ^greeting;
def_coord $s = ^spawn;
mes(tostring($a));
"""
    },
    "02-strings": {
        "strings.rs2": """[proc,strings]
def_int $n = 7;
def_string $name = "Bob";
def_string $t1 = "Hello <$name>, you have <tostring($n)> coins";
def_string $t2 = "<col=ff0000>red</col> and <br> break <u>u</u>";
def_string $t3 = "<p,quiz>Choose:";
def_string $t4 = "escaped \\" quote and \\\\ backslash and \\< lt";
def_string $t5 = "euro € sign and latin ñ and astral \U0001F600";
def_string $t6 = "nested <"inner <$name>">";
def_string $t7 = "tag then expr <col=00ff00><$t1></col>";
mes($t1);
mes($t2);
mes($t3);
mes($t4);
mes($t5);
mes($t6);
mes($t7);
"""
    },
    "03-control": {
        "control.rs2": """[proc,control]
def_int $x = random(10);
def_boolean $flag = true;
if ($x = 1) {
    mes("one");
} else if ($x ! 2 & $flag = true) {
    mes("not two and flag");
} else if ($x < 5 | $x > 8) {
    mes("outside");
} else if ((($x <= 6) | ($x >= 7)) & $flag = true) {
    mes("parens");
} else {
    mes("else");
}
while ($x > 0) {
    $x = calc($x - 1);
    if ($x = 3) {
        mes("three");
    }
}
switch_int ($x) {
    case 1 : mes("1");
    case 2, 3 : mes("2 or 3");
    case ^max_level : mes("max");
    case default : mes("default");
}
switch_boolean ($flag) {
    case true : mes("t");
    case false : mes("f");
}
switch_npc (man) {
    case man, woman : mes("human");
    case default : mes("other");
}
;
{
    def_int $inner = 1;
    mes(tostring($inner));
}
"""
    },
    "04-calc": {
        "calc.rs2": """[proc,arith]
def_int $a = 7;
def_int $b = 3;
def_int $c = calc($a + $b * 2 - 10 / 5 % 3);
def_int $d = calc(($a + $b) * (2 - $a));
def_int $e = calc($a & $b | 8);
def_int $f = calc(-5 + $a);
def_int $g = calc($a - 2147483647 - 2);
def_int $h = calc(0x7fffffff + 1);
def_int $i = calc(7 / -2);
def_int $j = calc(-7 % 3);
mes(tostring($c));
"""
    },
    "05-vars": {
        "vars.rs2": """[proc,vars]
def_int $i = 1;
def_string $s = "s";
def_int $j;
def_string $t;
%playtime = calc(%playtime + 1);
%name_var = "x";
.%playtime = %playtime;
%flag_bit = 1;
%world_counter = calc(%world_counter + 1);
%obj_charges = 1;
.%obj_charges = %obj_charges;
def_int $k = %flag_bit;
$i, $s = 2, "two";
$j = $i;
$t = $s;
mes($s);

[proc,arrays]
def_int $arr(10);
def_int $n = 5;
$arr(0) = 7;
$arr($n) = calc($arr(0) + 1);
def_int $x = $arr(3);
def_int $other(3);
$other(1) = $arr(1);
mes(tostring($arr(0)));
"""
    },
    "06-procs-labels": {
        "procs.rs2": """[proc,minmax](int $min, int $max, int $value)(int)
if ($max <= $min) {
    $min, $max = $max, $min;
}
if ($value < $min) {
    return($min);
}
if ($value > $max) {
    return($max);
}
return($value);

[proc,swap](int $a, string $b)(string, int)
return($b, $a);

[proc,tuples]
def_string $s;
def_int $i;
$s, $i = ~swap(1, "one");
mes($s);

[label,done](int $code)
mes(tostring($code));

[label,loop](int $n)
if ($n > 0) {
    @loop(calc($n - 1));
}
@done($n);

[proc,caller]
def_int $r = ~minmax(0, 10, 15);
~tuples;

[label,start]
~caller;
@loop(3);
"""
    },
    "07-commands": {
        "commands.rs2": """[proc,commands]
def_int $x = 5;
def_string $s = "str";
def_coord $c = 0_50_50_20_20;
def_npc $n = man;
if (npc_find($c, $n) = true) {
    npc_setvar(10);
    .npc_setvar(11);
}
def_int $total = inv_total(inv, coins);
dump($x, $s, coordx($c));
dump($total);
mes(escape($s));
"""
    },
    "08-macros": {
        "aa_defs.macro": """// macros collapse to a single line at the call site
#define SAY_TWICE, text
mes(text);
mes(text);
#enddef
#define ANSWER,
def_int $a = 42;
#enddef
""",
        # CRLF + macro calls ('#'): the preprocessor normalises CRLF -> LF outside strings/comments.
        "macros.rs2": b'[proc,macros]\r\n#ANSWER;\r\n#SAY_TWICE("hi");\r\nmes(tostring($a));\r\n',
        # CRLF without any '#': the file is not preprocessed and keeps the CRLF.
        "zz_crlf.rs2": b'[proc,crlf_kept]\r\nmes("no hash in this file so CRLF stays");\r\n',
    },
    "09-triggers": {
        "triggers.rs2": """[opnpc1,man]
mes("talking to man");

[opnpc1,_]
mes("any npc");

[debugproc,dbg](int $a, string $b, mapelement $c)
mes($b);
"""
    },
    "10-errors": {
        "errors.rs2": """[proc,errors]
def_int $a = "not an int";
def_string $s = 1;
mes($a);
undefined_command(1);
""",
        "syntax.rs2": """[proc,broken]
def_int $a = ;
mes("unterminated
""",
    },
}


# The client dialect (`.cs2`): its own command table (Jagex client opcodes from
# ares_shared::client_opcode) and tables. `cc_*` pointer rules: cc_create and
# cc_find set active_component, cc_deleteall has none, the rest require it.
CLIENT_COMMAND_IDS = {
    "cc_create": 100, "cc_delete": 101, "cc_deleteall": 102, "cc_find": 200,
    "cc_setposition": 1000, "cc_setsize": 1001, "cc_sethide": 1003, "cc_setcolour": 1101, "cc_settext": 1112,
    "cc_setonvartransmit": 1407, "cc_setontimer": 1408, "cc_getid": 1702,
    "if_sethide": 2003, "if_settext": 2112, "if_setonvartransmit": 2407, "if_setontimer": 2408, "if_setonop": 2409,
    "if_setoninvtransmit": 2414, "if_setonstattransmit": 2415,
    "mes": 3100, "clientclock": 3300, "inv_getobj": 3301, "inv_size": 3304, "stat": 3305,
    "tostring": 4106, "oc_name": 4200, "oc_param": 6512,
}
_cc = {str(i): n for n, i in CLIENT_COMMAND_IDS.items()}
_set = {str(i): "active_component" for n, i in CLIENT_COMMAND_IDS.items() if n in ("cc_create", "cc_find")}
_set2 = {k: "active_component2" for k in _set}
_req = {str(i): "active_component" for n, i in CLIENT_COMMAND_IDS.items() if n.startswith("cc_") and n not in ("cc_create", "cc_find", "cc_deleteall")}
_req2 = {k: "active_component2" for k in _req}
# cc_delete removes the active component.
_corrupt = {str(CLIENT_COMMAND_IDS["cc_delete"]): "active_component"}
_corrupt2 = {str(CLIENT_COMMAND_IDS["cc_delete"]): "active_component2"}

CLIENT_SYMBOLS = {
    "command": info(20000, _cc, set=_set, set2=_set2, require=_req, require2=_req2, corrupt=_corrupt, corrupt2=_corrupt2, conditional={str(CLIENT_COMMAND_IDS["cc_find"]): True}),
    "runescript": info(
        100,
        {
            "0": "[clientscript,wornitemstats_init]", "1": "[proc,wornitemstats_draw]", "2": "[clientscript,varc_ops]",
            "3": "[clientscript,events]", "4": "[clientscript,hooks]", "5": "[clientscript,dyn]",
            "6": "[clientscript,wornitemstats_redraw]", "7": "[clientscript,hook_errors]", "8": "[clientscript,hook_args]",
        },
    ),
    "constant": info(0, {"iftype_text": "4", "setpos_abs_left": "0", "setsize_abs": "0"}),
    "component": info(2, {"0": "wornitemstats:stats", "1": "wornitemstats:stabsatt"}),
    "interface": info(1, {"0": "wornitemstats"}),
    "inv": info(1, {"0": "worn"}),
    "obj": info(1, {"0": "bronze_sword"}),
    "param": info(1, {"0": "stabattack"}, vartype={"0": "int"}),
    "stat": info(1, {"0": "attack"}),
    "varp": info(1, {"0": "playtime"}, vartype={"0": "int"}, protect={"0": False}),
    "varbit": info(2, {"0": "flag_bit", "1": "shared_name"}, vartype={"0": "int", "1": "int"}, protect={"0": False, "1": False}),
    # `shared_name` is both a varbit and a varc: `%shared_name` is the varbit (varp, varbit, varc order).
    "varc": info(3, {"0": "tooltip_time", "1": "search_text", "2": "shared_name"}, vartype={"0": "int", "1": "string", "2": "string"}),
}

CLIENT_COMMANDS = """[command,cc_create](component $layer, int $type, int $sub)
[command,cc_delete]
[command,cc_deleteall](component $layer)
[command,cc_find](component $layer, int $sub)(boolean)
[command,cc_settext](string $text)
[command,cc_sethide](boolean $hide)
[command,cc_setposition](int $x, int $y, int $xmode, int $ymode)
[command,cc_setsize](int $w, int $h, int $wmode, int $hmode)
[command,cc_setcolour](int $colour)
[command,cc_getid]()(int)
[command,cc_setontimer](string $hook)
[command,cc_setonvartransmit](string $hook)
[command,if_settext](string $text, component $com)
[command,if_sethide](boolean $hide, component $com)
[command,if_setoninvtransmit](string $hook, component $com)
[command,if_setonvartransmit](string $hook, component $com)
[command,if_setonstattransmit](string $hook, component $com)
[command,if_setontimer](string $hook, component $com)
[command,if_setonop](string $hook, component $com)
[command,inv_getobj](inv $inv, int $slot)(obj)
[command,inv_size](inv $inv)(int)
[command,oc_param](obj $obj, param $param)(any)
[command,oc_name](obj $obj)(string)
[command,stat](stat $stat)(int)
[command,clientclock]()(int)
[command,tostring](int $value)(string)
[command,mes](string $text)
"""

CLIENT_CASES: dict[str, dict[str, str | bytes]] = {
    "11-client": {
        "client.cs2": """[clientscript,wornitemstats_init](component $stats)
~wornitemstats_draw($stats);
if_setoninvtransmit("wornitemstats_redraw($stats){worn}", $stats);

[clientscript,wornitemstats_redraw](component $stats)
~wornitemstats_draw($stats);

[proc,wornitemstats_draw](component $stats)
def_int $i = 0;
def_int $total = 0;
def_int $size = inv_size(worn);
while ($i < $size) {
    def_obj $obj = inv_getobj(worn, $i);
    if ($obj ! null) {
        $total = calc($total + oc_param($obj, stabattack));
    }
    $i = calc($i + 1);
}
if_settext("Stab: +<tostring($total)>", wornitemstats:stabsatt);

[clientscript,varc_ops]
%tooltip_time = clientclock;
%search_text = "abc";
if (%tooltip_time > 5) {
    mes(%search_text);
}
%playtime = 1;
def_int $shared = %shared_name;

[clientscript,events]
if_sethide(true, event_com);
mes(tostring(event_op));
mes(tostring(event_comsubid));
mes(tostring(event_mousex));
mes(tostring(event_keycode));

[clientscript,hooks]
if_setonvartransmit("varc_ops{playtime, flag_bit, tooltip_time}", wornitemstats:stats);
if_setonstattransmit("events{attack}", wornitemstats:stats);
if_setontimer("events", wornitemstats:stats);
if_setonop("wornitemstats_init(event_com)", wornitemstats:stats);
if_setonop(null, wornitemstats:stats);

[clientscript,dyn]
cc_deleteall(wornitemstats:stats);
cc_create(wornitemstats:stats, ^iftype_text, 0);
cc_setposition(0, 0, ^setpos_abs_left, ^setpos_abs_left);
cc_setsize(10, 10, ^setsize_abs, ^setsize_abs);
cc_settext("hi");
cc_setontimer("dyn");
if (cc_find(wornitemstats:stats, 0) = true) {
    cc_setcolour(0xff981f);
    cc_sethide(false);
}
.cc_create(wornitemstats:stats, ^iftype_text, 1);
.cc_settext("two");
cc_setonvartransmit("varc_ops{tooltip_time}");
def_string $text = "t";
cc_setontimer("hook_args(cc_getid, .cc_getid, event_opbase, $text)");

[clientscript,hook_args](int $a, int $b, string $s, string $t)
mes($s);
""",
    },
    "12-client-errors": {
        "client.cs2": """[clientscript,wornitemstats_init](component $stats)
mes("init");

[proc,wornitemstats_draw](component $stats)
mes("draw");

[clientscript,events]
mes("events");
""",
        "errors.cs2": """[clientscript,hook_errors]
if_setontimer("wornitemstats_draw(wornitemstats:stats)", wornitemstats:stats);
if_setontimer("wornitemstats_init(5)", wornitemstats:stats);
if_setontimer("events{attack}", wornitemstats:stats);
if_setonvartransmit("events{bronze_sword}", wornitemstats:stats);
def_string $hook = "events";
if_setontimer($hook, wornitemstats:stats);
if_setontimer("~wornitemstats_draw(wornitemstats:stats)", wornitemstats:stats);
""",
    },
}


def main() -> None:
    for case, files in CASES.items():
        d = ROOT / case / "scripts"
        d.mkdir(parents=True, exist_ok=True)
        (ROOT / case / "symbols.json").write_text(json.dumps(SYMBOLS, indent=1) + "\n", encoding="utf-8")
        (d / "aa_commands.rs2").write_text(COMMANDS, encoding="utf-8", newline="\n")
        for name, body in files.items():
            p = d / name
            if isinstance(body, bytes):
                p.write_bytes(body)
            else:
                p.write_text(body, encoding="utf-8", newline="\n")
    for case, files in CLIENT_CASES.items():
        d = ROOT / case / "scripts"
        d.mkdir(parents=True, exist_ok=True)
        (ROOT / case / "symbols.json").write_text(json.dumps(CLIENT_SYMBOLS, indent=1) + "\n", encoding="utf-8")
        (d / "aa_commands.cs2").write_text(CLIENT_COMMANDS, encoding="utf-8", newline="\n")
        for name, body in files.items():
            (d / name).write_text(body, encoding="utf-8", newline="\n")
    print(f"wrote {len(CASES) + len(CLIENT_CASES)} cases under {ROOT}")


if __name__ == "__main__":
    main()
