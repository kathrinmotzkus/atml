# ATML — Advanced TOML ("Atom-L")

A human-friendly, DRY-centric configuration language extending TOML v1.1.

Advanced TOML (ATML, spoken *ATOM-L*) extends standard TOML with native DRY
principles and domain-specific data types. Features hierarchical table
inheritance, bare path references, type-safe enums, and unquoted unit
quantities (e.g., `123ms`, `16GiB`). Fully backward-compatible baseline with
lossless flattening to standard TOML.

* **File extension:** `.atml`
* **MIME type:** `application/vnd.atml`
* **Baseline:** TOML v1.1.0

## Where ATML sits

ATML stays entirely within TOML's job: a declarative, readable format for
configuration *data*. It exists because plain TOML, at real scale, hits three
recurring pain points — and ATML adds exactly those three, nothing more:

* **Repetition.** TOML cannot share values between tables, so large or
  matrix-like configs repeat the same data again and again. Table inheritance
  lets a shared value be written once and inherited.
* **Units live outside the value.** A `350` is just a number; its unit survives
  only in a comment or baked into the key name. Quantities let a value carry its
  unit as data — `350km`, `1.80EUR/L`, `20m^3`.
* **Fixed-choice fields go unchecked.** A status or a mode is a bare string,
  with the allowed set living only in the author's head. Enums declare the
  choices once and check references against them.

Everything else about TOML is untouched: ATML is a strict superset, so every
valid TOML document is already valid ATML. And adopting it costs nothing
downstream — ATML **flattens to standard TOML**: the same data, in a file that
every existing TOML tool, in any language, reads unchanged. The convenience is
at authoring time; what you ship can be plain TOML.

## Features

* 🚀 **TOML v1.1 Baseline:** Every valid TOML 1.1 document is valid ATML.
* 🌿 **Inheritance:** Hierarchical table merging via `[child : parent]`,
  with multiple parents (`[child : p1, p2]`, first parent wins), for
  both standard tables and arrays of tables (`[[child : parent]]`).
* 🔗 **Bare Path References:** Assign values dynamically via
  `host = server.defaults.host`.
* 🏷️ **Type-Safe Enums:** Explicit enum references
  (`mode = SystemMode::Active`, namespaced via dotted keys: `net.Mode::Active`).
* ⏱️ **Mixed Quantities:** Native unquoted values with unit suffixes
  (`timeout = 123ms`, `limit = 16_384MiB`).
* 🔄 **Compliant Flattening:** Easily compiles back to 100% standard TOML.

## Example

```
[server.defaults]
read_timeout = 500ms
mode = OperationalMode::Active

[cache : server.defaults]
write_timeout = server.defaults.read_timeout
limit = 16_384MiB
```

## Repository layout

```
grammar/toml-1.1.0.abnf   Official TOML v1.1.0 ABNF, vendored byte-identical (MIT)
grammar/atml-ext.abnf     ATML extension rules (source of truth, purely additive)
grammar/atml.abnf         Built, self-contained grammar (do not edit directly)
tools/build_atml.py       Concatenates the grammar and enforces additivity
tests/test_grammar.py     Grammar test suite (positive/negative/regression)
tools/validate_atml.py    Validator: grammar + enum membership + inheritance
tests/test_validator.py   Validator test suite
SPEC.md                   Normative language specification
PARSER.md                 Conversion operations (flatten / re-flatten / lift)
ABNF.md                   Introduction to ABNF and the notation used here
EXAMPLE.md                Worked example (vehicle rental) — design and metrics
catalog/si-units.atml     SI units catalog, written in ATML
examples/vehicle-rental.atml   Worked example in ATML
examples/vehicle-rental.toml   The same fleet in plain TOML, for comparison
```

The extension file contains only `=/` incremental alternatives and new
`atml-*` rules. The superset property — every valid TOML 1.1.0 document is a
valid ATML document — is therefore proven by construction and enforced in CI.

## Building and testing

```
pip install abnf
python tools/build_atml.py
python tests/test_grammar.py
python tests/test_validator.py
```

## Validating a document

`tools/validate_atml.py` checks a document beyond raw grammar: enum
membership (a used symbol or value must belong to an in-scope enum),
inheritance parents (must be declared standard tables), and inheritance
cycles. It is a reference implementation and a conformance oracle for
other implementations.

```
python tools/validate_atml.py path/to/file.atml
```

## Relationship to TOML

ATML is a grateful extension of TOML, not a replacement. TOML, created by
Tom Preston-Werner, Pradyun Gedam, and contributors, is an excellent
configuration format, and ATML builds directly on it: the baseline grammar is
vendored unchanged, TOML interpretations always take precedence over ATML
ones, and tools that do not (yet) understand ATML can simply consume the
flattened standard TOML output.

> Configurations are written for humans, not for CPU registers.

## License

MIT — see [LICENSE](LICENSE). The vendored `grammar/toml-1.1.0.abnf` is
© Tom Preston-Werner, MIT-licensed; see `grammar/TOML-LICENSE`.
