#!/usr/bin/env python3
"""ATML validator (reference implementation, Python).

Checks an .atml document for validity beyond raw grammar conformance. It is a
companion to the flattener and a conformance oracle for other implementations
(e.g. the Rust toml_dom crate): given the same input, it should report the
same diagnostics.

Checks performed (all based on decisions recorded in SPEC.md):
  1. Grammar conformance   -- the whole document parses against atml.abnf.
  2. Enum membership       -- SPEC section 5 (#18, #19):
       a) a reference <enum-name>::<symbol> names a declared enum that is in
          scope (declared earlier in the document) and whose symbol set
          contains <symbol>;
       b) a direct use <key> = <value>, where <key> is declared as an enum in
          scope, uses a <value> that is in that enum's value set.
  3. Inheritance parents   -- SPEC section 4 (#14): every parent of an
       inheriting table (standard or array-of-tables) must resolve to a
       declared standard table (not an array-of-tables, not undefined).
  4. Inheritance cycles    -- SPEC section 6 (#20): the child->parents graph
       must be acyclic.

Not yet covered (these need the resolution engine that comes with the
flattener): resolving bare path references to their target values, and
detecting reference cycles.

BINDING RULES (see SPEC section 5):
  - An enum must be declared before it is used (definition-before-use).
  - A reference `Name::symbol` names its enum explicitly.
  - A plain value use `port = 20` names no enum; it is linked to an enum by
    the key name -- a declaration `port[] = [...]` above binds every later
    `port = X`, and 20 is then checked against the declared values.
  - This key-name binding is global (by decision): a top-level `port[]`
    applies to every `port =` below it, regardless of the table it sits in.
    This lets admins declare standard enums once at the top of a document.

Inheritance, by contrast, is order-independent: a parent table may be declared
before or after the child (SPEC section 4), matching standard TOML tables.

Requires: pip install abnf
"""
from __future__ import annotations

import sys
import warnings
from dataclasses import dataclass, field
from pathlib import Path

import abnf

# The vendored toml.abnf redefines ALPHA/DIGIT/HEXDIG identically to the RFC
# core rules; the resulting warnings are harmless.
warnings.filterwarnings("ignore", category=abnf.GrammarWarning)

ROOT = Path(__file__).resolve().parent.parent
GRAMMAR = ROOT / "grammar" / "atml.abnf"


@dataclass
class Diagnostic:
    line: int
    message: str

    def __str__(self) -> str:
        where = f"line {self.line}" if self.line else "?"
        return f"{where}: {self.message}"


@dataclass
class _Enum:
    name: str
    symbols: set[str] = field(default_factory=set)
    values: set[str] = field(default_factory=set)
    line: int = 0


def _load_grammar() -> type:
    src = GRAMMAR.read_text(encoding="ascii").replace("\r\n", "\n").replace("\n", "\r\n")

    class ATML(abnf.Rule):
        pass

    ATML.load_grammar(src)
    return ATML


class _LineFinder:
    """Best-effort line lookup that advances a cursor in document order."""

    def __init__(self, source: str) -> None:
        self._source = source
        self._cursor = 0

    def line_of(self, text: str) -> int:
        idx = self._source.find(text, self._cursor)
        if idx < 0:
            idx = self._source.find(text)  # fall back to first occurrence
        if idx < 0:
            return 0
        self._cursor = idx + max(len(text), 1)
        return self._source.count("\n", 0, idx) + 1


def _named(node, name):
    """Yield descendant nodes with the given rule name (depth-first)."""
    if getattr(node, "name", None) == name:
        yield node
    for child in getattr(node, "children", []):
        yield from _named(child, name)


def _first_key(node) -> str | None:
    for k in _named(node, "key"):
        return k.value.strip()
    return None


def _keys(node) -> list[str]:
    return [k.value.strip() for k in _named(node, "key")]


def _last_segment(dotted: str) -> str:
    return dotted.split(".")[-1].strip()


def validate(source: str) -> list[Diagnostic]:
    """Return a list of diagnostics; empty means the document is valid."""
    grammar = _load_grammar()
    norm = source.replace("\r\n", "\n").replace("\n", "\r\n")

    # --- Check 1: grammar conformance -------------------------------------
    try:
        tree = grammar("toml").parse_all(norm)
    except abnf.ParseError as exc:
        offset = getattr(exc, "start", None)
        line = norm.count("\n", 0, offset) + 1 if isinstance(offset, int) else 0
        return [Diagnostic(line, "syntax error: document does not parse as ATML")]

    lf = _LineFinder(source)
    diags: list[Diagnostic] = []

    # --- Build model in document order ------------------------------------
    enums: dict[str, _Enum] = {}
    std_tables: set[str] = set()
    array_tables: set[str] = set()
    inherit_edges: list[tuple[str, list[str], int]] = []

    # We walk top-level expressions in order so declarations precede uses.
    for expr in getattr(tree, "children", []):
        _walk_collect(expr, lf, enums, std_tables, array_tables,
                      inherit_edges, diags)

    # --- Check 3 & 4 need the full table sets, run after collection -------
    _check_inheritance(inherit_edges, std_tables, array_tables, diags)

    diags.sort(key=lambda d: (d.line, d.message))
    return diags


def _walk_collect(node, lf, enums, std_tables, array_tables, inherit_edges, diags):
    name = getattr(node, "name", None)

    if name == "atml-enum-decl":
        ename = _first_key(node)
        line = lf.line_of(node.value.split("\n")[0])
        e = _Enum(name=ename, line=line)
        lists = list(_named(node, "atml-enum-list"))
        if lists:
            for sym in _named(lists[0], "atml-enum-symbol"):
                e.symbols.add(sym.value.strip())
            for val in getattr(lists[0], "children", []):
                pass  # values gathered below
            # value choices are 'val' nodes directly under the list
            for val in _named(lists[0], "val"):
                # avoid capturing nested vals inside inline tables/arrays:
                e.values.add(val.value.strip())
        enums[ename] = e
        return  # nothing deeper to collect here

    if name == "std-table":
        std_tables.add(_first_key(node))
        return
    if name == "array-table":
        array_tables.add(_first_key(node))
        return
    if name == "atml-inherit-table":
        ks = _keys(node)
        std_tables.add(ks[0])
        inherit_edges.append((ks[0], ks[1:], lf.line_of(node.value.split("\n")[0])))
        return
    if name == "atml-inherit-array":
        ks = _keys(node)
        array_tables.add(ks[0])
        inherit_edges.append((ks[0], ks[1:], lf.line_of(node.value.split("\n")[0])))
        return

    if name == "atml-enum-ref":
        ks = _keys(node)
        ename = ks[0]
        syms = list(_named(node, "atml-enum-symbol"))
        symbol = syms[0].value.strip() if syms else ""
        line = lf.line_of(node.value)
        _check_enum_ref(ename, symbol, line, enums, diags)
        return

    if name == "keyval":
        # A direct value use: <key> = <value>. If the key is a declared enum
        # (in scope), the value must be in the enum's value set.
        # Skip if this keyval is itself an enum declaration or its value is a
        # reference/enum-ref (handled elsewhere).
        if not list(_named(node, "atml-enum-decl")) and not list(_named(node, "atml-enum-ref")):
            key = _first_key(node)
            vals = [c for c in getattr(node, "children", []) if getattr(c, "name", None) == "val"]
            if key is not None and vals:
                value_text = vals[0].value.strip()
                line = lf.line_of(node.value.split("\n")[0])
                _check_value_use(key, value_text, line, enums, diags)
        # keep walking into children for nested constructs
        for child in getattr(node, "children", []):
            _walk_collect(child, lf, enums, std_tables, array_tables, inherit_edges, diags)
        return

    for child in getattr(node, "children", []):
        _walk_collect(child, lf, enums, std_tables, array_tables, inherit_edges, diags)


def _check_enum_ref(ename, symbol, line, enums, diags):
    e = enums.get(ename)
    if e is None or e.line > line:
        diags.append(Diagnostic(line, f"enum '{ename}' is not declared in scope"))
        return
    if symbol not in e.symbols:
        allowed = ", ".join(sorted(e.symbols)) or "(none)"
        diags.append(Diagnostic(
            line, f"'{symbol}' is not a symbol of enum '{ename}' (allowed: {allowed})"))


def _check_value_use(key, value_text, line, enums, diags):
    e = enums.get(key)
    if e is None or e.line > line:
        return  # key is not a declared enum in scope -> ordinary keyval
    if value_text not in e.values:
        allowed = ", ".join(sorted(e.values)) or "(none)"
        diags.append(Diagnostic(
            line, f"{value_text} is not an allowed value of enum '{key}' (allowed: {allowed})"))


def _check_inheritance(inherit_edges, std_tables, array_tables, diags):
    # Parents must resolve to declared standard tables.
    for child, parents, line in inherit_edges:
        for p in parents:
            if p in std_tables:
                continue
            if p in array_tables:
                diags.append(Diagnostic(
                    line, f"parent '{p}' is an array-of-tables; parents must be standard tables"))
            else:
                diags.append(Diagnostic(line, f"parent '{p}' is not a declared table"))

    # Cycle detection over child -> parents.
    graph: dict[str, list[str]] = {}
    for child, parents, _ in inherit_edges:
        graph.setdefault(child, []).extend(parents)

    WHITE, GREY, BLACK = 0, 1, 2
    color: dict[str, int] = {}

    def dfs(node: str, stack: list[str]) -> bool:
        color[node] = GREY
        stack.append(node)
        for nxt in graph.get(node, []):
            if color.get(nxt, WHITE) == GREY:
                cycle = " -> ".join(stack[stack.index(nxt):] + [nxt])
                diags.append(Diagnostic(0, f"inheritance cycle: {cycle}"))
                return True
            if color.get(nxt, WHITE) == WHITE and nxt in graph:
                if dfs(nxt, stack):
                    return True
        stack.pop()
        color[node] = BLACK
        return False

    for node in list(graph):
        if color.get(node, WHITE) == WHITE:
            dfs(node, [])


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: validate_atml.py <file.atml>", file=sys.stderr)
        return 2
    source = Path(argv[1]).read_text(encoding="utf-8")
    diags = validate(source)
    if not diags:
        print("valid")
        return 0
    for d in diags:
        print(d)
    print(f"\n{len(diags)} problem(s) found.")
    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
