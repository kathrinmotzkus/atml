#!/usr/bin/env python3
"""Tests for the ATML validator. Requires: pip install abnf"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tools"))
import validate_atml as v  # noqa: E402


def messages(source: str) -> list[str]:
    return [d.message for d in v.validate(source)]


def is_valid(source: str) -> bool:
    return not v.validate(source)


CASES = []


def case(name, source, expect_valid, must_contain=None):
    CASES.append((name, source, expect_valid, must_contain))


# --- valid documents ------------------------------------------------------
case("clean symbol enum",
     "Strategy[] = [Active, Passive]\r\n"
     "mode = Strategy::Active\r\n",
     True)

case("clean value enum (scanlab ports)",
     "port[] = [110, 111, 143]\r\n"
     "[[service]]\r\n"
     "port = 111\r\n",
     True)

case("mixed enum, symbol + value both used",
     'level[] = [off, 0, 1, 2]\r\n'
     'a = level::off\r\n'
     'b = 2\r\n'
     'b = 0\r\n',   # 'b' is not the enum key, so not checked; still valid
     True)

case("inheritance valid",
     "[server.base]\r\n"
     "os = 1\r\n"
     "[server.prod : server.base]\r\n"
     "x = 2\r\n",
     True)

case("inheritance order-independent (parent below child)",
     "[server.prod : server.base]\r\n"
     "x = 2\r\n"
     "[server.base]\r\n"
     "os = 1\r\n",
     True)

# --- enum membership failures --------------------------------------------
case("unknown enum in reference",
     "mode = Strategy::Active\r\n",
     False, "is not declared in scope")

case("symbol not in enum",
     "Strategy[] = [Active, Passive]\r\n"
     "mode = Strategy::Audit\r\n",
     False, "is not a symbol of enum 'Strategy'")

case("value not in value-enum (wrong exam answer)",
     "port[] = [110, 111, 143]\r\n"
     "[[service]]\r\n"
     "port = 25\r\n",
     False, "is not an allowed value of enum 'port'")

case("enum used before declared (out of scope)",
     "mode = Strategy::Active\r\n"
     "Strategy[] = [Active, Passive]\r\n",
     False, "is not declared in scope")

# --- inheritance failures -------------------------------------------------
case("inherit from undeclared parent",
     "[child : missing]\r\n"
     "x = 1\r\n",
     False, "is not a declared table")

case("inherit from array-of-tables",
     "[[pool]]\r\n"
     "n = 1\r\n"
     "[child : pool]\r\n"
     "x = 2\r\n",
     False, "is an array-of-tables")

case("inheritance cycle",
     "[a : b]\r\n"
     "x = 1\r\n"
     "[b : a]\r\n"
     "y = 2\r\n",
     False, "inheritance cycle")

# --- run ------------------------------------------------------------------
fails = 0
for name, source, expect_valid, must_contain in CASES:
    valid = is_valid(source)
    ok = (valid == expect_valid)
    if ok and not expect_valid and must_contain is not None:
        ok = any(must_contain in m for m in messages(source))
    if not ok:
        fails += 1
        print(f"FAIL [{name}] expected_valid={expect_valid} got_valid={valid}")
        for m in messages(source):
            print(f"      - {m}")

print(f"{len(CASES) - fails}/{len(CASES)} validator tests passed.")
raise SystemExit(1 if fails else 0)
