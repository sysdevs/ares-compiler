"""Write tests/pointers/inputs.json: pointer-checking cases (same format and
gate as tests/pipeline). Commands carry pointer metadata
(required / set / conditional set / corrupted, primary and `.` forms); random
scripts on triggers with different implicit pointers call them directly, through
procs (incl. recursion), through labels and `jump(label)`, inside conditions
(normal and inverted conditional setters) and loops.

usage (from ares-compiler/): python -I tools/make_pointer_corpus.py
"""
import json
import random
from pathlib import Path

rng = random.Random(20261022)


def info(m, **extra):
    base = {k: {} for k in ["vartype", "protect", "require", "require2", "conditional", "set", "set2", "corrupt", "corrupt2"]}
    base.update(extra)
    return {"max": 100, "map": m, **base}


CMDS = ["mes", "npc_find", "npc_findnext", "npc_del", "p_delay", "loc_find", "loc_del", "p_teleport", "huntall",
        "npc_huntnext", "jump", "npc_say", "obj_add", "p_opnpc", "tostring", "inv_slotobj"]
# Command ids start above the core opcodes (0..=100), like real ones.
IDX = {n: str(1000 + i) for i, n in enumerate(CMDS)}
SYMBOLS = {
    "command": info({IDX[n]: n for n in CMDS},
                    require={IDX["npc_del"]: "active_npc", IDX["p_delay"]: "p_active_player", IDX["loc_del"]: "active_loc",
                             IDX["p_teleport"]: "p_active_player", IDX["npc_say"]: "active_npc", IDX["p_opnpc"]: "active_player,active_npc",
                             IDX["obj_add"]: "active_player", IDX["npc_find"]: "none", IDX["inv_slotobj"]: "active_player"},
                    require2={IDX["npc_del"]: "active_npc2", IDX["npc_say"]: "active_npc2", IDX["inv_slotobj"]: "active_player2"},
                    set={IDX["inv_slotobj"]: "active_invobj", IDX["npc_find"]: "active_npc", IDX["npc_findnext"]: "active_npc", IDX["loc_find"]: "active_loc",
                         IDX["npc_huntnext"]: "active_npc"},
                    set2={IDX["npc_find"]: "active_npc2", IDX["inv_slotobj"]: "active_invobj2"},
                    conditional={IDX["inv_slotobj"]: True, IDX["npc_find"]: True, IDX["npc_findnext"]: True, IDX["npc_huntnext"]: True},
                    corrupt={IDX["huntall"]: "active_npc", IDX["p_delay"]: "active_loc"},
                    corrupt2={IDX["huntall"]: "active_npc2"}),
    "runescript": info({"0": "[proc,dummy]"}),
    "npc": info({"0": "man"}),
    "component": info({"0": "bank:close", "1": "overlay:x", "2": "Over Lay:y"}),
    "interface": info({"0": "bank"}),
    "overlayinterface": info({"0": "overlay", "1": "Over Lay"}),
    "varp": info({"0": "vp", "1": "vp_prot"}, vartype={"0": "int", "1": "int"}, protect={"1": True}),
    "varn": info({"0": "vn"}, vartype={"0": "int"}),
    "varo": info({"0": "vo"}, vartype={"0": "int"}),
    "obj": info({"0": "coins"}),
    "inv": info({"0": "bag"}),
}
PRELUDE = """[command,mes](string $s)
[command,npc_find](coord $c, npc $n, int $r)(boolean)
[command,npc_findnext]()(boolean)
[command,npc_del]
[command,p_delay](int $n)
[command,loc_find]
[command,loc_del]
[command,p_teleport]
[command,huntall]
[command,npc_huntnext]()(boolean)
[command,jump](label $l)
[command,npc_say](string $s)
[command,obj_add]
[command,p_opnpc]
[command,tostring](int $n)(string)
[command,inv_slotobj](inv $i, int $s)(boolean)
"""

LEAVES = ["npc_del;", ".npc_del;", "p_delay(1);", "loc_find;", "loc_del;", "p_teleport;", "huntall;", "npc_say(\"x\");",
          ".npc_say(\"y\");", "obj_add;", "p_opnpc;", "mes(\"m\");", "%vp = 1;", "%vp_prot = 2;", ".%vp = 3;", ".%vp_prot = 4;",
          "mes(tostring(%vp));", "%vn = 1;", "mes(tostring(.%vn));", "npc_findnext;", "npc_find(0_50_50_0_0, man, 5);",
          "%vo = 1;", "mes(tostring(.%vo));", "inv_slotobj(bag, 0);"]


def cond():
    c = rng.choice(["npc_find(0_50_50_0_0, man, 5) = true", "npc_findnext = true", "npc_huntnext = true", "inv_slotobj(bag, 0) = true",
                    "npc_find(0_50_50_0_0, man, 5) = false", "npc_findnext = false", "%vp = 1", "1 = 1"])
    if rng.random() < 0.25:
        c = f"{c} {rng.choice(['&', '|'])} {cond()}"
    return c


def stmt(calls, d=2):
    r = rng.random()
    if d > 0 and r < 0.2:
        return f"if ({cond()}) {{ {block(calls, d - 1)} }}" + (f" else {{ {block(calls, d - 1)} }}" if rng.random() < 0.4 else "")
    if d > 0 and r < 0.3:
        return f"while ({cond()}) {{ {block(calls, d - 1)} }}"
    if d > 0 and r < 0.35:
        return f"switch_int (1) {{ case 1 : {block(calls, d - 1)} case default : {block(calls, d - 1)} }}"
    if calls and r < 0.55:
        return rng.choice(calls)
    return rng.choice(LEAVES)


def block(calls, d):
    return " ".join(stmt(calls, d) for _ in range(rng.randint(0, 3)))


def case():
    n = rng.randint(1, 4)
    procs = [f"p{i}" for i in range(n)]
    labels = [f"l{i}" for i in range(rng.randint(0, 2))]
    calls = [f"~{p};" for p in procs] + [f"@{l};" for l in labels] + [f"~takes_label({l});" for l in labels] + [f"jump({l});" for l in labels]
    scripts = []
    for p in procs:
        scripts.append(f"[proc,{p}] {block(calls, 2)}")
    for l in labels:
        scripts.append(f"[label,{l}] {block([c for c in calls if not c.startswith('@')], 2)}")
    if labels:
        scripts.append("[proc,takes_label](label $l) " + rng.choice(["jump($l);", "if (1 = 1) { jump($l); }", "p_delay(1); jump($l);", "mes(\"no jump\");"]))
    for _ in range(rng.randint(1, 3)):
        trigger = rng.choice(["[opnpc1,man]", "[ai_spawn,man]", "[softtimer,st]", "[login,_]", "[if_button1,bank:close]",
                              "[if_button1,overlay:x]", "[inv_button1,overlay:x]", "[if_button,bank:close]", "[if_button,overlay:x]", "[if_button2,Over Lay:y]", "[queue,qq]",
                              "[opheld1,coins]", "[apnpc1,_]", "[timer,tt]", "[walktrigger,wt]"])
        scripts.append(f"{trigger} {block([c for c in calls if not c.startswith('@')], 2)}")
        # one script per trigger/subject: dedupe by trigger text
        heads = [s.split(']')[0] for s in scripts]
        if len(set(heads)) != len(heads):
            scripts.pop()
    return "\n".join(scripts)


# Client dialect: cc_create/cc_find set active_component, cc_settext requires it.
CLIENT_CMDS = {"cc_create": "100", "cc_find": "200", "cc_settext": "1112"}
CLIENT_SYMBOLS = {
    "command": info({v: k for k, v in CLIENT_CMDS.items()},
                    set={"100": "active_component", "200": "active_component"},
                    set2={"100": "active_component2", "200": "active_component2"},
                    conditional={"200": True},
                    require={"1112": "active_component"},
                    require2={"1112": "active_component2"}),
    "runescript": info({"0": "[clientscript,ok]", "1": "[clientscript,bad]", "2": "[clientscript,dot]"}),
    "component": info({"0": "bank:close"}),
}
CLIENT_PRELUDE = """[command,cc_create](component $layer, int $type, int $sub)
[command,cc_find](component $layer, int $sub)(boolean)
[command,cc_settext](string $text)
"""
CLIENT_CASES = [
    "[clientscript,ok] cc_create(bank:close, 4, 0); cc_settext(\"x\");",
    "[clientscript,bad] cc_settext(\"x\");",
    "[clientscript,dot] .cc_create(bank:close, 4, 1); .cc_settext(\"x\"); cc_settext(\"y\");",
]


def main():
    inputs = []
    for i in range(400):
        features = {} if i % 4 else {"pointerInversion": False}
        inputs.append({"symbols": SYMBOLS, "features": features, "files": [{"name": "aa_prelude.rs2", "text": PRELUDE}, {"name": "case.rs2", "text": case()}]})
    for text in CLIENT_CASES:
        inputs.append({"symbols": CLIENT_SYMBOLS, "features": {}, "dialect": "client",
                       "files": [{"name": "aa_prelude.cs2", "text": CLIENT_PRELUDE}, {"name": "case.cs2", "text": text}]})
    out = Path("tests/pointers/inputs.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(inputs, ensure_ascii=False, indent=0) + "\n", encoding="utf-8")
    print(f"{len(inputs)} cases -> {out}")


main()
