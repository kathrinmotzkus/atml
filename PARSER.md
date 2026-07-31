# ATML Parser & Conversion Operations

**Status:** Design in progress.

**Scope:** This document covers converting between ATML and TOML. These are
properties of the parser/converter (implemented in Python), **not** of the ATML
or TOML *languages* — the same ATML could be lowered differently by different
tools without changing either language. The language itself is defined in
`SPEC.md`; this file never adds to or changes it.

## 1. Three operations

Three distinct conversions are kept separate:

- **Flattening** — ATML → TOML. Mechanical and exact.
- **Re-flattening** — TOML that carries round-trip hints → ATML. Mechanical;
  best-effort, guided by the hints.
- **Lifting** (working name) — arbitrary TOML *without* hints → ATML.
  Heuristic and suggestive; its output is a proposal for review.

## 2. Flattening (ATML → TOML)

The flattener expands inheritance, resolves references, and lowers enums and
quantities. The emitted output must validate against the official, unmodified
`toml.abnf` — no ATML construct may leak into the live data. Detecting
reference and inheritance cycles is a required part of the resolution pass, and
a reference sees the value *after* inheritance has been resolved.

### 2.1 Quantity lowering

A quantity `<number><unit>` lowers to an inline table
`{ value = <number>, unit = "<unit>" }`. Because an inline table is itself a
value, this works in every position a quantity may appear — as a keyval value,
an array element, or an inline-table field. `value` takes its natural TOML
number type (`123`, `1.5`, `16_384`); `unit` is a quoted string. Examples:

- `timeout = 123ms` → `timeout = { value = 123, unit = "ms" }`
- `limits = [512MiB, 1GiB]` →
  `limits = [ { value = 512, unit = "MiB" }, { value = 1, unit = "GiB" } ]`

A quantity that carries a **super-unit** (rate / product, SPEC §2 #7) lowers to
two additional fields: `separator` (the `/` or `*`, recorded because it
distinguishes a rate from a product) and `super_unit`. An **exponent** stays
inside the unit token as-is; the lowerer does not interpret it (consistent with
"ATML validates unit syntax, not semantics"). When there is no super-unit, the
`separator` and `super_unit` fields are omitted. Examples:

- `price = 1.80EUR/L` →
  `price = { value = 1.80, unit = "EUR", separator = "/", super_unit = "L" }`
- `torque = 5N*m` →
  `torque = { value = 5, unit = "N", separator = "*", super_unit = "m" }`
- `area = 20m^3` → `area = { value = 20, unit = "m^3" }`
- `accel = 9.81m/s^2` →
  `accel = { value = 9.81, unit = "m", separator = "/", super_unit = "s^2" }`

### 2.2 Enum symbol-reference lowering

A reference `<enum-name>::<symbol>` lowers to the symbol as a quoted string,
e.g. `mode = Strategy::Active` → `mode = "Active"`. A direct value use
(`port = 111`) is already valid TOML and is emitted unchanged.

### 2.3 Inheritance expansion

Inheritance is expanded by merging: a child is filled with its parents'
resolved key/values (first-wins across multiple parents), then its own
key/values override. The result is emitted as an ordinary `[table]` or
`[[array]]` with no `:` construct remaining.

### 2.4 Meta-construct lowering

Constructs with no standard-TOML equivalent — enum declarations, and template
tables once identified — are lowered to valid TOML but preserved as round-trip
hints (§3) rather than living in the main file. By default they are preserved,
not dropped.

**Template tables — a configurable flattener policy.** Which tables count as
templates is not hard-wired; the flattener offers the user a choice of policy
when it runs:

- **`auto` (default):** a table used anywhere as an inheritance parent is
  treated as a template — its keys are merged into its descendants, it is
  preserved as a hint (§3), and it is not emitted as a live table. Concrete
  tables (never a parent) are emitted live. For the vehicle example this yields
  a main file containing only the resolved fleet. A **per-table override** lets
  the user force an individual table to be kept live or treated as a template
  when `auto` misjudges the rare table that is both a parent and real data.
- **`emit-all`:** every table is emitted live with its resolved values,
  templates included. Safest (nothing is ever suppressed), but the output
  carries the template scaffolding.
- **`explicit`:** nothing is auto-detected; the user names the tables or
  namespaces that are templates, and only those are treated as such.

Regrouping the data along its natural shared axis often removes the need for a
template entirely, and remains a good first modelling instinct.

## 3. Round-trip hints (the sidecar file)

Every lowered construct is hinted — quantities, enum references, expanded
inheritance, resolved path references, enum declarations, and template tables —
so reconstruction is exact rather than guessed. To keep the flattened
`<name>.toml` readable, the flattener does **not** interleave `#[atml]` markers
throughout the main file. Instead:

- `<name>.toml` stays clean and carries a single header comment, e.g.

  ```
  # This file was flattened from <name>.atml. Round-trip hints: <name>.conv.toml
  ```

- The `#[atml] … [/atml]` blocks that record the original ATML are collected in
  a companion file, `<name>.conv.toml`, which the main file names in its header.

**Association is by key path, not by line position.** A hint for
`server.defaults.timeout` maps to that key in the main file wherever it sits.
So if a user edits the value —

```
timeout = { value = 234, unit = "s" }
```

— the key `timeout` still resolves and its hint is still found by path. The
match is on the key, not the line.

**The `.conv.toml` is a *hint* file, not a guarantee.** It records "how it was
before"; it does not force an exact reconstruction. The live `<name>.toml` is
the source of truth, and the hints give context for a best-effort re-flattening
with human review. This deliberately steps back from a 100%-guaranteed
round-trip without abandoning it — and it keeps the concern where it belongs:
in the parser, not in either language.

**Edge case:** if a user renames or deletes a key, its hint no longer has a
target and is simply unused — harmless, because hints never override the live
data.

### 3.1 Marker syntax

The hints use a reserved bracket pair `[atml] … [/atml]`, always inside TOML
comments (every line begins with `#`), so any file that contains them stays
valid standard TOML.

- **Multi-line:** an opening line `#[atml]`, the original ATML on the following
  comment lines (each begins with `#`), and a closing line `#[/atml]`.
- **Single-line:** `#[atml] <original ATML> [/atml]` on one comment line.

A reverse converter recognizes the `[atml]` / `[/atml]` pair to locate the
embedded ATML; plain TOML tools treat the whole region as ordinary comments.
Example:

```
#[atml]
#   Strategy[] = [Active, Passive]
#[/atml]
```

## 4. Re-flattening (TOML → ATML)

Re-flattening reconstructs ATML from a flattened `<name>.toml` together with its
`<name>.conv.toml` hints. It reads the `[atml] … [/atml]` regions, restores the
recorded ATML for each key path, and rebuilds ATML constructs from the live
values where they map cleanly. Because the hints are context rather than
authority, the result is a best-effort reconstruction: where a value was edited
by hand, the live value wins and the hint informs only the shape.

## 5. Lifting (arbitrary TOML → ATML)

Distinct from re-flattening: lifting converts *arbitrary* TOML that carries no
hints. It cannot rely on markers, so it must *discover* structure — repeated
key groups across tables suggest inheritance, repeated values suggest
references or enums — and *propose* a DRY form. Example input:

```
[server.http]
os = "linux"
timeout = 30
[server.mail]
os = "linux"
timeout = 30
```

A lifting tool might propose a shared base the two tables inherit from. This is
heuristic and non-unique: its output is a *suggestion for review*, not a
guaranteed original. It is therefore neither flattening nor re-flattening.

## 6. Open questions

- **Build lifting, and name it?** Whether to implement the arbitrary-TOML→ATML
  tool, and its final name (candidates: *lifting*, *DRY-ification*).
- **Template identification** (resolved, see §2.4): a configurable flattener
  policy — `auto` (parent-used = template, the default, with per-table
  override), `emit-all`, or `explicit`.
- **Super-unit lowering field names** (resolved, see §2.1): `value`, `unit`,
  `separator`, `super_unit`; the exponent stays inside the unit token.
- **Hint granularity** (resolved): the flattener hints **every** lowered
  construct — quantities, enum references, expanded inheritance, resolved path
  references, plus the enum declarations and template tables. The rule is
  trivially "always", reconstruction is exact rather than best-effort guessing,
  and the sidecar becomes a key-path-indexed trace of the original ATML — which
  matches its role as "how it was before".

*Resolved by the sidecar design of §3:* the earlier questions about
inline-marker noise, the source of truth after a manual edit, and inline-marker
placement. Hints now live in `<name>.conv.toml`, and the live file is the
source of truth.
