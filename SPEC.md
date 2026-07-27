# ATML Specification

**Status:** Draft 1 (grammar complete, resolution semantics in progress)
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
   quantities.
8. **ATML validates syntax, not the semantics of units.** Units are defined
   by the author of the file, not by ATML or TOML. Unusual but collision-free
   units (e.g. `0xenon`) are grammatically valid; style concerns belong in a
   linter, not in the grammar.

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

Syntax: `[child : parent]` or `[child : parent1, parent2, ...]`.

11. Child and parents use the standard TOML `key` rule (bare, quoted, or
    dotted). A colon outside quotes is impossible in standard TOML headers,
    so the syntax is collision-free; `["a:b"]` remains an ordinary TOML
    table name.
12. **Conflict resolution: first wins.** When multiple parents define the
    same key, earlier parents take precedence; the child overrides all
    parents. The parent list is a priority list.
13. **Reserved:** `[[child : parent]]` (array-of-tables inheritance) is
    reserved and currently invalid.

## 5. Type-Safe Enums

Syntax: `mode = OperationalMode::Active`, namespaced: `net::Mode::Active`.

14. An enum has **at least two segments** separated by `::`; multi-level
    namespaces are allowed.
15. Segments follow Rust identifier conventions: they start with a letter or
    underscore, followed by letters, digits, or underscores — no hyphens.

## 6. Flattening

16. `.atml` documents compile losslessly to standard TOML: inheritance is
    expanded, references are resolved, enums and quantities are lowered to
    their configured standard representations. The emitted output must
    validate against the official, unmodified `toml.abnf` — no ATML
    construct may leak through. Detection of reference cycles is a required
    part of the resolution pass. (Detailed resolution semantics: in
    progress.)
