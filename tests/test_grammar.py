#!/usr/bin/env python3
"""Test suite for grammar/atml.abnf. Requires: pip install abnf"""
import warnings
from pathlib import Path

import abnf

warnings.filterwarnings("ignore")

ROOT = Path(__file__).resolve().parent.parent
src = (ROOT / "grammar" / "atml.abnf").read_text(encoding="ascii")
src = src.replace("\r\n", "\n").replace("\n", "\r\n")


class ATML(abnf.Rule):
    pass


ATML.load_grammar(src)


def accepts(rule: str, text: str) -> bool:
    try:
        ATML(rule).parse_all(text)
        return True
    except abnf.ParseError:
        return False


VAL_CASES = [
    # --- Standard TOML regression: everything must remain valid ---
    ("0xFF", True), ("0xfade", True), ("0o755", True), ("0b1101", True),
    ("123", True), ("1.5", True), ("inf", True), ("nan", True),
    ("true", True), ("3.14159", True), ("1979-05-27", True), ("07:32", True),
    # --- Feature 1: Mixed Quantities ---
    ("123ms", True), ("0ms", True), ("0dB", True), ("0bar", True), ("0ohm", True),
    ("16_384MiB", True), ("1.5s", True), ("1e3Hz", True), ("-40dB", True), ("+0K", True),
    ("0xenon", True), ("0xms", True), ("0xFFGG", True),  # collision-free x-units
    ("0x", False),
    ("infms", False), ("nanGiB", False), ("07ms", False),
    # --- Feature 2: Bare Path References ---
    ("server.defaults.read_timeout", True), ("a.b", True), ("_p.k", True),
    ("db.conn-pool.max-size", True),
    ("defaults", False), ("a.3", False), ("a..b", False), ("a. b", False),
    # --- Enums: reference <enum-name>::<symbol> (exactly one "::") ---
    ("Strategy::Passive", True), ("probe.Strategy::Passive", True), ("_m::_v", True),
    ("Strategy::", False), ("::Passive", False), ("Strategy::2fast", False),
    ("Strategy::Fast-Lane", False), ("probe::Strategy::Passive", False),
    # --- Enums: markerless definition (value side); needs >=1 bare symbol ---
    ("[Active, Passive]", True), ("[Active]", True), ("[ tcp , udp ]", True),
    ('[variant1, "variant2", false, 3.14]', True),
]

# Marked enum declaration is a whole keyval line (LHS carries "[]").
KEYVAL_CASES = [
    ("port[] = [110, 111, 143]", True),
    ("Strategy[] = [Active, Passive]", True),
    ('mixed[] = [variant1, "variant2", false, 3.14]', True),
    ("empty[] = []", False),            # at least one choice required
    ("plain = [1, 2, 3]", True),        # ordinary keyval still fine
]

TABLE_CASES = [
    # --- Feature 3: Table Inheritance ---
    ("[cache : server]", True), ("[c : a, b]", True), ("[c:a,b]", True),
    ("[prod.db : defaults.db]", True), ('[c : "srv one", b]', True),
    ('["a:b"]', True), ("[server]", True), ("[[products]]", True),
    ("[a : ]", False), ("[ : b]", False),
    # --- Array-of-tables inheritance (decision #13, implemented) ---
    ("[[tcp : proto.rpc]]", True), ("[[tcp : proto.base, proto.rpc]]", True),
    ("[[tcp:proto.rpc]]", True), ('[["a:b"]]', True),
    ("[[tcp : ]]", False), ("[[ : proto.rpc]]", False),
]

FULL_ATML_DOC = (
    "# Integration of all four features\r\n"
    "Strategy = [Active, Passive]\r\n"
    "\r\n"
    "[server.defaults]\r\n"
    "read_timeout = 500ms\r\n"
    "mode = Strategy::Active\r\n"
    "port = 0xFF\r\n"
    "\r\n"
    "[cache : server.defaults]\r\n"
    "write_timeout = server.defaults.read_timeout\r\n"
    "limit = 16_384MiB\r\n"
    "\r\n"
    "[edge : server.defaults, cache]\r\n"
    "mode = Strategy::Passive\r\n"
)

TOML_REGRESSION_DOC = (
    'title = "test"\r\npi = 3.14159\r\nnothing = nan\r\n'
    '[a]\r\nx = 0xDEAD_BEEF\r\nwhen = 1979-05-27 07:32Z\r\n'
    '[["b:c"]]\r\ny = [ 1, { k = true } ]\r\n'
)

fails = 0
for text, expected in VAL_CASES:
    got = accepts("val", text)
    if got != expected:
        fails += 1
        print(f"FAIL val   {text!r}: expected={expected} got={got}")
for text, expected in TABLE_CASES:
    got = accepts("table", text)
    if got != expected:
        fails += 1
        print(f"FAIL table {text!r}: expected={expected} got={got}")
for text, expected in KEYVAL_CASES:
    got = accepts("keyval", text)
    if got != expected:
        fails += 1
        print(f"FAIL keyval {text!r}: expected={expected} got={got}")
for name, doc in [("full ATML document", FULL_ATML_DOC),
                  ("TOML regression document", TOML_REGRESSION_DOC)]:
    if not accepts("toml", doc):
        fails += 1
        print(f"FAIL {name} does not parse")

total = len(VAL_CASES) + len(TABLE_CASES) + len(KEYVAL_CASES) + 2
print(f"{total - fails}/{total} tests passed.")
raise SystemExit(1 if fails else 0)
