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

## Features

* 🚀 **TOML v1.1 Baseline:** Every valid TOML 1.1 document is valid ATML.
* 🌿 **Inheritance:** Hierarchical table merging via `[child : parent]`,
  with multiple parents (`[child : p1, p2]`, first parent wins).
* 🔗 **Bare Path References:** Assign values dynamically via
  `host = server.defaults.host`.
* 🏷️ **Type-Safe Enums:** Explicit enum references
  (`mode = SystemMode::Active`, namespaced: `net::Mode::Active`).
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
SPEC.md                   Normative specification
```

The extension file contains only `=/` incremental alternatives and new
`atml-*` rules. The superset property — every valid TOML 1.1.0 document is a
valid ATML document — is therefore proven by construction and enforced in CI.

## Building and testing

```
pip install abnf
python tools/build_atml.py
python tests/test_grammar.py
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
