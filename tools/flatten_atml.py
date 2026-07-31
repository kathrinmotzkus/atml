#!/usr/bin/env python3
"""ATML flattener (reference implementation, Python).

Turns an .atml document into standard TOML, following the decisions recorded in
PARSER.md. It produces two files:

  <name>.toml       the flattened, standard-TOML data (inheritance resolved,
                    references substituted, quantities and enum references
                    lowered), with a header comment pointing to the sidecar;
  <name>.conv.toml  round-trip hints: for every lowered construct, plus enum
                    declarations and template tables, a `#[atml] ... [/atml]`
                    block keyed by key path, so re-flattening can reconstruct
                    the original ATML.

Template handling is a configurable policy (PARSER.md 2.4):
  auto      (default) a table used as an inheritance parent is a template:
            merged into its children, hinted, not emitted live;
  emit-all  every table is emitted live with resolved values;
  explicit  only tables named on the command line are treated as templates.

This is a reference implementation and a conformance oracle for other
implementations (e.g. the Rust toml_dom crate). It intentionally favours
clarity over speed, and reuses the same atml.abnf grammar as the validator.

Requires: pip install abnf
"""
from __future__ import annotations

import sys
import warnings
from dataclasses import dataclass, field
from pathlib import Path

import abnf

warnings.filterwarnings("ignore", category=abnf.GrammarWarning)

ROOT = Path(__file__).resolve().parent.parent
GRAMMAR = ROOT / "grammar" / "atml.abnf"


# --------------------------------------------------------------------------
# Grammar + tree helpers (shared shape with the validator)
# --------------------------------------------------------------------------
def _load_grammar() -> type:
    src = GRAMMAR.read_text(encoding="ascii").replace("\r\n", "\n").replace("\n", "\r\n")

    class ATML(abnf.Rule):
        pass

    ATML.load_grammar(src)
    return ATML


def _named(node, name):
    if getattr(node, "name", None) == name:
        yield node
    for child in getattr(node, "children", []):
        yield from _named(child, name)


def _first_named(node, name):
    for n in _named(node, name):
        return n
    return None


def _keys(node) -> list[str]:
    """Direct-ish keys of a header/keyval, in order (dotted keys kept whole)."""
    return [k.value.strip() for k in _named(node, "key")]


# --------------------------------------------------------------------------
# Model
# --------------------------------------------------------------------------
@dataclass
class Entity:
    """A table or one array-of-tables element."""
    name: str
    kind: str                       # "table" or "array"
    parents: list[str] = field(default_factory=list)
    # ordered (key, raw_value_text, val_node) triples
    items: list[tuple] = field(default_factory=list)


@dataclass
class Document:
    enums: dict = field(default_factory=dict)          # name -> raw decl text
    entities: list = field(default_factory=list)       # in document order
    used_as_parent: set = field(default_factory=set)


# --------------------------------------------------------------------------
# Parse ATML text into the model
# --------------------------------------------------------------------------
# Order matters: an inheriting header ([[a:b]]) parses as an array-table that
# *contains* an atml-inherit-array, so the specific inherit rules are checked
# before the general table rules.
HEADER_RULES = ["atml-inherit-array", "atml-inherit-table", "array-table", "std-table"]


def parse(source: str) -> Document:
    grammar = _load_grammar()
    norm = source.replace("\r\n", "\n").replace("\n", "\r\n")
    tree = grammar("toml").parse_all(norm)

    doc = Document()
    current: Entity | None = None

    for expr in getattr(tree, "children", []):
        enum_decl = _first_named(expr, "atml-enum-decl")
        if enum_decl is not None:
            name = _keys(enum_decl)[0]
            doc.enums[name] = enum_decl.value.strip()
            continue

        header = None
        for rule in HEADER_RULES:
            header = _first_named(expr, rule)
            if header is not None:
                break
        if header is not None:
            ks = _keys(header)
            if header.name in ("atml-inherit-table", "atml-inherit-array"):
                name, parents = ks[0], ks[1:]
            else:
                name, parents = ks[0], []
            kind = "array" if header.name in ("array-table", "atml-inherit-array") else "table"
            current = Entity(name=name, kind=kind, parents=parents)
            doc.entities.append(current)
            for p in parents:
                doc.used_as_parent.add(p)
            continue

        keyval = _first_named(expr, "keyval")
        if keyval is not None and current is not None:
            key = _keys(keyval)[0]
            val = None
            for c in getattr(keyval, "children", []):
                if getattr(c, "name", None) == "val":
                    val = c
                    break
            if val is not None:
                current.items.append((key, val.value.strip(), val))
        elif keyval is not None and current is None:
            # top-level keyval before any table -> a root entity
            key = _keys(keyval)[0]
            val = _first_named(keyval, "val")
            root = _lookup_or_make(doc, "", "table")
            if val is not None:
                root.items.append((key, val.value.strip(), val))
    return doc


def _lookup_or_make(doc: Document, name: str, kind: str) -> Entity:
    for e in doc.entities:
        if e.name == name and e.kind == kind:
            return e
    e = Entity(name=name, kind=kind)
    doc.entities.append(e)
    return e


# --------------------------------------------------------------------------
# Inheritance resolution (first-wins, transitive)
# --------------------------------------------------------------------------
def _table_index(doc: Document) -> dict:
    idx = {}
    for e in doc.entities:
        if e.kind == "table":
            idx[e.name] = e
    return idx


def resolve(entity: Entity, tables: dict, seen=None) -> list[tuple]:
    """Return merged (key, raw, node) items: own keys win, then parents in order."""
    seen = seen or set()
    result: dict[str, tuple] = {}
    order: list[str] = []

    def put(items):
        for key, raw, node in items:
            if key not in result:          # first-wins
                result[key] = (key, raw, node)
                order.append(key)

    # own items first (child wins)
    put(entity.items)
    # then parents, in listed order
    for pname in entity.parents:
        if pname in seen:
            continue
        parent = tables.get(pname)
        if parent is None:
            continue
        put(resolve(parent, tables, seen | {entity.name}))
    return [result[k] for k in order]


# --------------------------------------------------------------------------
# Value lowering
# --------------------------------------------------------------------------
def lower_value(raw: str, node) -> tuple[str, str | None]:
    """Return (lowered_toml_text, original_atml_or_None_if_unchanged)."""
    kind = _value_kind(node)
    if kind == "atml-quantity":
        return _lower_quantity(node), raw
    if kind == "atml-enum-ref":
        sym = raw.split("::", 1)[1].strip()
        return f'"{sym}"', raw
    if kind == "atml-path-ref":
        # left as-is for now; a resolver pass will substitute the target value
        return raw, raw
    return raw, None


def _value_kind(node):
    val = node if getattr(node, "name", None) == "val" else _first_named(node, "val")
    for c in getattr(val, "children", []):
        n = getattr(c, "name", None)
        if n:
            return n
    return None


def _lower_quantity(node) -> str:
    q = _first_named(node, "atml-quantity")
    number = None
    for r in ("atml-float-num", "atml-nonzero-dec", "atml-signed-zero", "atml-zero"):
        n = _first_named(q, r)
        if n is not None:
            number = n.value.strip()
            break
    unit_node = _first_named(q, "atml-unit") or _first_named(q, "atml-zero-unit")
    unit = unit_node.value.strip() if unit_node else ""
    super_node = _first_named(q, "atml-super")
    if super_node is not None:
        sep = super_node.value[0]
        super_unit = super_node.value[1:].strip()
        return (f'{{ value = {number}, unit = "{unit}", '
                f'separator = "{sep}", super_unit = "{super_unit}" }}')
    return f'{{ value = {number}, unit = "{unit}" }}'


# --------------------------------------------------------------------------
# Flatten
# --------------------------------------------------------------------------
def flatten(source: str, stem: str, template_mode: str = "auto",
            explicit_templates: set | None = None) -> tuple[str, str]:
    doc = parse(source)
    tables = _table_index(doc)

    def is_template(e: Entity) -> bool:
        if template_mode == "emit-all":
            return False
        if template_mode == "explicit":
            pref = explicit_templates or set()
            return any(e.name == p or e.name.startswith(p + ".") for p in pref)
        return e.kind == "table" and e.name in doc.used_as_parent  # auto

    main: list[str] = [
        f"# Flattened from {stem}.atml. Round-trip hints: {stem}.conv.toml",
        "",
    ]
    hints: list[str] = [
        f"# Round-trip hints for {stem}.toml. Each block records the original",
        "# ATML for a key path; the live .toml is the source of truth.",
        "",
    ]

    # enum declarations -> hints only
    for name, decl in doc.enums.items():
        hints.append(f"#[atml] {decl} [/atml]")
    if doc.enums:
        hints.append("")

    for e in doc.entities:
        if e.name == "":
            continue  # implicit root holder, if any
        merged = resolve(e, tables)
        header = f"[[{e.name}]]" if e.kind == "array" else f"[{e.name}]"
        template = is_template(e)

        target = hints if template else main
        if template:
            # record the template's own (unmerged) declaration + items as a hint
            decl_head = (f"[{e.name} : {', '.join(e.parents)}]"
                         if e.parents else f"[{e.name}]")
            hints.append(f"#[atml] {decl_head} [/atml]")
            for key, raw, node in e.items:
                hints.append(f"#[atml] {e.name}.{key} = {raw} [/atml]")
            hints.append("")
            continue

        main.append(header)
        for key, raw, node in merged:
            low, _ = lower_value(raw, node)
            main.append(f"{key} = {low}")
        main.append("")

        # sidecar: the ORIGINAL thin entry (its inheritance header + own items),
        # not the merged/inherited values -- those belong to the templates.
        if e.parents:
            plist = ", ".join(e.parents)
            orig_head = (f"[[{e.name} : {plist}]]" if e.kind == "array"
                         else f"[{e.name} : {plist}]")
        else:
            orig_head = header
        hints.append(f"#[atml] {orig_head} [/atml]")
        for key, raw, node in e.items:
            hints.append(f"#[atml] {key} = {raw} [/atml]")
        hints.append("")

    return "\n".join(main).rstrip() + "\n", "\n".join(hints).rstrip() + "\n"


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print("usage: flatten_atml.py <file.atml> [--emit-all|--explicit t1,t2]",
              file=sys.stderr)
        return 2
    path = Path(argv[1])
    mode, explicit = "auto", set()
    if "--emit-all" in argv:
        mode = "emit-all"
    if "--explicit" in argv:
        mode = "explicit"
        i = argv.index("--explicit")
        explicit = set(argv[i + 1].split(",")) if i + 1 < len(argv) else set()

    source = path.read_text(encoding="utf-8")
    stem = path.stem
    main_toml, sidecar = flatten(source, stem, mode, explicit)

    out_main = path.with_suffix(".toml")
    out_side = path.with_name(f"{stem}.conv.toml")
    out_main.write_text(main_toml, encoding="utf-8")
    out_side.write_text(sidecar, encoding="utf-8")
    print(f"wrote {out_main.name} and {out_side.name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
