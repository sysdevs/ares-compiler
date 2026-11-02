"""Write tests/registration/inputs.json: script-registration cases (triggers,
subjects, parameters, returns, redeclarations, feature flags) built from seeded
random script headers plus hand-picked ones.

usage (from ares-compiler/): python -I tools/make_register_corpus.py
"""
import json
import random
from pathlib import Path

rng = random.Random(20261019)


def info(m, **extra):
    base = {k: {} for k in ["vartype", "protect", "require", "require2", "conditional", "set", "set2", "corrupt", "corrupt2"]}
    base.update(extra)
    return {"max": 100, "map": m, **base}


SYMBOLS = {
    "command": info({"0": "mes", "1": "npc_setvar", "2": "p_delay", "3": "seqtest"},
                    require={"1": "active_npc", "2": "active_player", "3": "none"}, require2={"1": "active_npc2"}, set={"3": ""}),
    "runescript": info({"0": "[proc,a]", "1": "[label,b]"}),
    "constant": info({"max": "99"}),
    "npc": info({"0": "man", "1": "Big Rat", "2": "goblin"}),
    "obj": info({"0": "coins", "1": "bones"}),
    "category": info({"0": "animals", "1": "weapons"}),
    "loc": info({"0": "door"}),
    "component": info({"0": "bank:close"}),
    "stat": info({"0": "attack"}),
    "inv": info({"0": "bank"}),
    "enum": info({"0": "e1"}),
    "dbrow": info({"0": "row1"}),
}

TRIGGERS = ["proc", "label", "command", "debugproc", "opnpc1", "apnpc1", "opheld1", "apnpct", "oploc1", "mapzone", "zone",
            "ai_spawn", "login", "logout", "softtimer", "walktrigger", "queue", "timer", "advancestat", "if_button1",
            "inv_button1", "nope", "PROC", "opobj1", "clientscript"]
SUBJECTS = ["_", "_animals", "_weapons", "_nocat", "man", "Man", "big rat", "Big  Rat", "goblin", "coins", "door",
            "bank:close", "attack", "0_50_50", "1_50_50", "0_300_50", "0_x_50", "0_50", "0_50_50_8_16", "0_50_50_3_8",
            "4_50_50_8_8", "0_50_50_8", "a b", "x", "a.b", "0_50_50_-8_8"]
PARAMS = ["", "()", "(int $a)", "(intarray $a, string $b)", "(int $a, int $a)", "(boolean $b)", "(enum $e)",
          "(dbrow $r)", "(long $l)", "(namedobj $o)", "(coord $c)", "(nope $x)", "(varp $v)", "(stringarray $s)",
          "(queue $q)", "(int $a, string $if)", "(booleanarray $bs)", "(category $c)"]
RETURNS = ["", "()", "(int)", "(int,string)", "(intarray)", "(nope)", "(boolean)", "(long)", "(enum)"]
FEATURES = [{}, {}, {}, {"procs": False}, {"booleans": False, "enums": False, "structs": False, "dbtables": False}]


def header():
    trigger = rng.choice(TRIGGERS)
    subject = rng.choice(SUBJECTS) + rng.choice(["", "", "", "*"])
    params = rng.choice(PARAMS)
    returns = rng.choice(RETURNS) if params else ""
    return f"[{trigger},{subject}]{params}{returns}"


EDGE = [
    "[proc,a]\n[proc,a]\n[label,a]\n[command,mes](string $text)\n[command,mes*](string $t)",
    "[command,npc_setvar](int $v)\n[command,p_delay]\n[opnpc1,_animals]\n[opnpc1,man]\n[opnpc1,man]",
    "[mapzone,0_50_50]\n[mapzone,0_50_50_1]\n[zone,0_50_50_8_16]\n[zone,0_50_50_7_8]\n[zone,0_x_50_8_8]",
    "[apnpct,_]\n[apnpct,_x]\n[login,_]\n[login,x]\n[login,_cat]\n[proc,a b]\n[opnpc1,big rat]",
]


def main():
    inputs = [{"symbols": SYMBOLS, "features": {}, "files": [{"name": "edge.rs2", "text": t} for t in EDGE]}]
    for _ in range(300):
        files = []
        for j in range(rng.randint(1, 3)):
            files.append({"name": f"f{j}.rs2", "text": "\n".join(header() + rng.choice(["", " return;", " ;"]) for _ in range(rng.randint(1, 6)))})
        inputs.append({"symbols": SYMBOLS, "features": rng.choice(FEATURES), "files": files})
    out = Path("tests/registration/inputs.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(inputs, ensure_ascii=False, indent=0) + "\n", encoding="utf-8")
    print(f"{len(inputs)} cases -> {out}")


main()
