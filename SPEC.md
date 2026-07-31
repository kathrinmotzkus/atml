# ATML Specification

**Grammar:** Stable. The syntax of all five feature areas — quantities, bare
path references, table inheritance, array-of-tables inheritance, and enums —
is complete, implemented in `grammar/atml.abnf`, and covered by the test suite.
**Scope:** This document defines the ATML *language*. Conversion between ATML
and TOML (flattening, re-flattening, lifting) is a parser/tool concern and
lives in `PARSER.md`.
**Baseline:** TOML v1.1.0 (released 2025-12-18)

This document records the normative decisions of the ATML language. The
grammar (`grammar/atml.abnf`) defines syntax; several rules below are
semantic and intentionally not expressible in ABNF.

## 1. Foundation

1. **Baseline.** ATML is defined on top of TOML v1.1.0. The official ABNF is
   vendored byte-identical as `grammar/toml-1.1.0.abnf` and pinned. Upgrades
   to future TOML versions are deliberate, selective decisions of the ATML
   project — never automatic.
2. **Additivity invariant.** The extension grammar contains only `=/`
   incremental alternatives to existing baseline rules and new rules whose
   names start with `atml-`. Every valid TOML 1.1.0 document is therefore a
   valid ATML document, by construction. `tools/build_atml.py` enforces this
   invariant and CI verifies the vendored baseline against upstream.
3. **Precedence rule (normative).** If a value parses as a standard TOML
   value, it *is* a standard TOML value. TOML interpretations always take
   precedence over ATML interpretations. Example: `0xFF` is the hexadecimal
   integer 255, never a quantity.
4. **Identity.** File extension `.atml`; MIME type `application/vnd.atml`.
   Both belong exclusively to the ATML project.

## 2. Mixed Quantities

Syntax: a decimal number immediately followed by a unit, e.g. `timeout = 123ms`,
`limit = 16_384MiB`, `gain = -40dB`, `warmup = 1.5s`, `rate = 1e3Hz`.
A quantity may additionally carry an optional **super-unit** for rates and
products, e.g. `price = 1.80EUR/L`, `energy = 40EUR/kWh`, `torque = 5N*m`.

5. **Decimal base only.** The numeric base is a decimal integer (including
   underscores and sign) or a numeric float. Hexadecimal, octal, and binary
   literals are excluded (prefix exclusion), as are `inf` and `nan`
   (no `special-float`).
6. **`nan` and `inf`** remain valid as plain TOML floats (superset promise)
   but are forbidden as quantity bases: `nanGiB` and `infms` are invalid.
7. **Units are letters only** (`1*ALPHA`). After a bare `0`, a unit may start
   with `x` only if at least one non-hex letter (`g-z`, `G-Z`) follows —
   a precise, positively encoded collision exclusion with hexadecimal
   literals, requiring no lookahead. `b` and `o` need no exclusion because
   binary and octal literals require digits; `0bar` and `0ohm` are valid
   quantities. **Optional super-unit (rate / product):** after the unit a
   quantity may carry a separator and a further unit —
   `<value><unit>[<sep><super-unit>]`. The separator is `/` ("per") or `*`
   ("times"), e.g. `1.80EUR/L`, `40EUR/kWh`, `100km/h`, `5N*m`. **Exponents:**
   a unit (and a super-unit) may carry an exponent in two interchangeable forms
   — a caret `^` with optional sign and digits (`m^2`, `m^3`, `s^-1`,
   `9.81m/s^2`) or a Unicode superscript (`m²`, `m³`, `m⁻¹`, `9.81m/s²`). The
   caret form keeps digits behind `^`, so a bare unit never gains a digit and
   the hexadecimal collision stays impossible; superscripts are non-ASCII and
   equally collision-free. All of this stays collision-free overall: `/`, `*`,
   and `^` occur in no TOML value, and a unit must still begin with a letter, so
   `1/2` and `2024/01` are not quantities.
8. **ATML validates syntax, not the semantics of units.** Units, separators, and
   super-units — everything after `<value>` — are defined by the author, not by
   ATML or TOML; unusual but collision-free units (e.g. `0xenon`) are
   grammatically valid, and style concerns belong in a linter. The allowed set
   of units, separators, and super-units *may* additionally be constrained by
   author-defined enums (e.g. `Separator[] = ["/", "*"]`; separators are quoted
   string values, since `/` is not a bare symbol). That constraint is a semantic
   layer, **not** enforced by the grammar; it is recommended where a parser
   benefits from a fixed, validated set, and tools should document whether they
   apply it.

**Partly resolved — special characters in units.** Exponents are now covered
(see #7): area, volume, and rates like `m²`/`m^2`, `m³`/`m^3`, `m/s²`/`m/s^2`
are expressible. What remains **open** is non-ASCII **symbols inside unit
names** — the micro prefix `µ`, `Ω`, `°`, `Å`, and subscripts such as the `₂`
in `SpO₂`. These currently must be spelled with letters (`u`, `ohm`, `degC`,
`SpO2`-style). Allowing them raises a Unicode-in-units question, and several are
hard to type — an authoring concern as much as a grammar one. Plain **digits**
directly inside a unit (`m2`, `m3`) remain disallowed on purpose: exponents use
the caret or superscript instead, which keeps the hexadecimal collision
(`0x1F`) impossible. This symbol question is deliberately left open.

## 3. Bare Path References

Syntax: `write_timeout = server.defaults.read_timeout`.

9. A reference has **at least two segments**. Each segment starts with a
   letter or underscore, followed by letters, digits, underscores, or
   hyphens. No whitespace around the dots (deliberately stricter than TOML
   keys). This keeps `nan`, `true`, `inf` (single tokens) and `3.14159`
   (digit segments) unambiguously standard TOML values.
10. **Known limitation.** Purely numeric keys (`3.14159`) and keys requiring
    quoting (`"127.0.0.1"`) cannot be addressed by bare references. A future
    quoted-segment extension remains possible without breaking this grammar.

## 4. Table Inheritance

Syntax: `[child : parent]` / `[child : parent1, parent2, ...]` for standard
tables, and `[[child : parent]]` / `[[child : parent1, parent2, ...]]` for
arrays of tables.

11. Child and parents use the standard TOML `key` rule (bare, quoted, or
    dotted). A colon outside quotes is impossible in standard TOML headers,
    so the syntax is collision-free; `["a:b"]` and `[["a:b"]]` remain
    ordinary TOML table names.
12. **Conflict resolution: first wins.** When multiple parents define the
    same key, earlier parents take precedence; the child overrides all
    parents. The parent list is a priority list. Resolution is transitive:
    parents are resolved recursively first, so a table inherits from its
    parents' parents automatically. A chained header form (`[a : b : c]`) is
    therefore deliberately not provided — transitivity already carries the
    chain, and multiple direct parents are written with commas.
13. **Array-of-tables inheritance.** `[[child : parent]]` appends a new
    element to the array `child`, pre-seeded with the fully resolved
    key/values of its parents (parents are resolved recursively first, so
    transitive inheritance applies), then overridden by the element's own
    key/values. Multiple parents follow the same first-wins rule as #12.
    Sub-tables and sub-arrays declared afterwards (e.g. `[[child.sub]]`)
    attach positionally to the most recently created element, unchanged from
    standard TOML.
14. **Parents must resolve to standard tables.** A parent is referenced by
    name, and array-of-tables elements are not name-addressable (they are
    indexed positionally). Therefore the parent of any inheriting table —
    standard or array — must resolve to a standard table. This is a semantic
    rule; the grammar does not and cannot enforce it. **Table order is not
    significant:** a parent may be declared before or after the inheriting
    table, matching standard TOML's order-independent tables. This is a
    deliberate contrast with enums, which follow definition-before-use (§19).

## 5. Enums

An enum is a datatype with a fixed set of predefined choices. A choice is
either a bare symbol (an identifier such as `Active`) or an ordinary TOML
value (a string, number, boolean, …). Both kinds may be mixed in one enum.

There are two definition forms:

Marked (universal): `<enum-name>[] = [ choice, choice, ... ]`
Markerless (shorthand): `<enum-name> = [ choice, choice, ... ]`

15. **Marked declaration.** The `[]` marker on the key makes the entry an enum
    regardless of its contents. Because `[` and `]` cannot occur in a TOML
    key, the form is collision-free and additive. It is the general form and
    the only *reliable* way to declare a list of purely ordinary values (e.g.
    ports `[110, 111, 143]`): such a list is otherwise indistinguishable from
    a plain array, so the markerless form below would be demoted to an array.
16. **Markerless declaration.** Valid ATML as well, but it acts as an enum
    only when the list does not parse as a plain TOML array — that is, when it
    contains at least one bare symbol. A list of purely ordinary values stays
    a plain array under the precedence rule of section 4. The grammar is
    permissive here; that precedence rule performs the demotion.
17. **Choice form.** A bare symbol starts with a letter or underscore, then
    letters, digits, or underscores (no hyphen). An ordinary value is any
    standard TOML value. A definition list holds at least one choice.
18. **Use.** A symbol is referenced as `<enum-name>::<symbol>` with exactly one
    `::`. An ordinary value is used directly (e.g. `port = 111`). Since a TOML
    key cannot contain `::`, namespacing uses ordinary dotted keys, e.g.
    `probe.Strategy::Passive` for an enum declared as `probe.Strategy[] = [ … ]`.
    (This supersedes the earlier multi-`::` reference form; `a::b::c` is no
    longer valid.)
19. **Membership (normative, not grammar).** A used symbol or value must belong
    to the enum's definition, and the definition must be in scope (present at a
    higher position in the document). A direct value use is linked to its enum
    by **key name, globally**: a declaration `port[] = [ … ]` binds every later
    `port = …` regardless of the table it appears in, so admins can declare
    standard enums once at the top of a document. This is a binding across
    distance that a context-free grammar cannot express; it lives in this spec
    text, not in `atml.abnf` — analogous to TOML's "a key may not be defined
    twice".
