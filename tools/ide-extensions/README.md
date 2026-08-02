# ATML IDE extensions

This directory contains the editor tooling for ATML. The first supported editor
is Visual Studio Code; the language intelligence itself lives in Rust and is
exposed through the Language Server Protocol (LSP), so it can later be reused by
other editors.

The implementation sequence and acceptance criteria are documented in the
[development plan](PLAN.md).

## Architecture

```text
VS Code
└── vscode-atml (TypeScript)
    ├── registers .atml files
    ├── provides immediate lexical highlighting
    └── starts and communicates with the Rust language server
                         │ stdio / LSP
                         ▼
atml-language-server (Rust)
└── translates LSP requests and responses
                         │
                         ▼
atml-language-core (Rust)
├── parses TOML 1.1 and ATML with toml_dom 0.4
├── maintains document snapshots and source positions
├── builds the semantic ATML model
└── provides diagnostics, completion, hover and navigation
```

The VS Code extension contains no second ATML parser. Syntactic and semantic
truth comes from `atml-language-core` and ultimately `toml_dom`. The TextMate
grammar in the client is only responsible for fast, provisional coloring while
the language server starts.

## Repository structure

```text
ide-extensions/
├── README.md
├── FORMATTING.md                # format-preserving formatter contract
├── FUZZING.md                   # coverage-guided robustness testing
├── RELEASING.md                 # six-target VSIX release procedure
├── rust-toolchain.toml          # pinned release toolchain
├── Cargo.toml                   # Rust workspace
├── crates/
│   ├── atml-language-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── document.rs      # immutable document snapshots
│   │       ├── syntax.rs        # toml_dom adapter and source ranges
│   │       ├── semantic.rs      # enums, references and inheritance
│   │       ├── diagnostics.rs
│   │       ├── completion.rs
│   │       ├── navigation.rs    # hover, definitions and references
│   │       └── editing.rs       # semantic tokens, rename and quick fixes
│   └── atml-language-server/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── server.rs        # LSP lifecycle and capabilities
│           └── documents.rs     # open document/version management
└── vscode-atml/
    ├── package.json
    ├── README.md                # Marketplace documentation
    ├── CHANGELOG.md
    ├── LICENSE
    ├── images/icon.png
    ├── scripts/                 # package, normalize and verify VSIX
    ├── tsconfig.json
    ├── language-configuration.json
    ├── syntaxes/atml.tmLanguage.json
    ├── src/extension.ts
    └── test/
```

Generated files such as Rust `target/`, Node `node_modules/`, VS Code `.vsix`
packages and bundled language-server binaries must not be committed.

## Responsibilities

### `vscode-atml`

- Associate `*.atml` files with the `atml` language identifier.
- Define comments, brackets, folding pairs, auto-closing pairs and indentation.
- Provide TextMate highlighting for TOML plus ATML quantities, enum declarations,
  enum references, path references and inherited table headers.
- Start the native language-server binary over standard input/output.
- Forward configuration changes and expose restart/log commands.
- Package the correct server binary for each supported platform at release time.

### `atml-language-server`

- Implement LSP initialization, shutdown and document synchronization.
- Keep versioned in-memory text snapshots; never parse an older version after a
  newer edit has arrived.
- Convert the core's byte offsets into UTF-16 LSP positions.
- Advertise only capabilities that are actually implemented.
- Keep protocol and transport concerns out of the semantic core.

### `atml-language-core`

- Depend on `toml_dom = "0.4"` from crates.io.
- Parse ATML with `Document::parse_atml` and strict TOML where comparison is
  useful for diagnostics.
- Preserve authored source structure through the `toml_dom` CST/DOM.
- Build indexes for keys, tables, enum definitions, enum uses, references and
  inheritance relationships.
- Perform semantic checks that are intentionally outside the ABNF parser:
  enum definition-before-use, enum membership, reference targets, inheritance
  targets and cycles.
- Return editor-neutral results with source ranges; it must not depend on VS
  Code APIs or LSP data types.

## Feature stages

### Stage 1: usable foundation

1. Open and synchronize `*.atml` documents.
2. Parse through `toml_dom 0.4` after each debounced edit.
3. Report syntax errors with line, column and a clear message.
4. Add TOML-compatible highlighting plus the ATML lexical additions.
5. Provide document symbols for keys and tables.

### Stage 2: ATML understanding

1. Build the semantic index for enums, references and inheritance.
2. Diagnose unknown references, cycles, invalid enum members and use before
   definition.
3. Offer completion for keys, reference paths, enum names and enum members.
4. Provide hover information for quantities, resolved references, enums and
   inherited values.
5. Implement go-to-definition for references, enums and parent tables.

### Stage 3: editing support

1. Find references and rename symbols safely.
2. Add semantic tokens where TextMate highlighting is insufficient.
3. Add code actions for common errors.
4. Add formatting only after its exact interaction with format-preserving
   `toml_dom` editing has been specified.

## Diagnostics model

Diagnostics should be divided into stable categories so clients can filter and
test them:

| Category | Examples |
|---|---|
| Syntax | malformed quantity, enum reference or inherited header |
| TOML semantics | duplicate key, conflicting table definition |
| ATML binding | missing enum/reference/parent definition |
| ATML type | enum member not declared by the referenced enum |
| ATML graph | cyclic path reference or cyclic table inheritance |

Every diagnostic needs a stable code such as `atml.syntax.expected-unit` or
`atml.enum.unknown-member`; tests should assert codes and ranges rather than the
complete prose message.

## Completion contexts

Completion is context-sensitive rather than a global list:

| Cursor context | Suggestions |
|---|---|
| value after `=` | TOML values, visible enum names and reference roots |
| after `Enum::` | members declared by that enum |
| inside a bare path | reachable keys below the resolved path prefix |
| after `:` in a table header | existing standard tables |
| quantity suffix | known units; later optionally project-defined units |

The first version should derive suggestions solely from the current document.
Workspace-wide symbols and schema-based completion can be added without
changing the protocol boundary.

## Testing strategy

- Core unit tests use the examples and conformance cases from this ATML
  repository as fixtures.
- Parser behavior is tested against `grammar/atml.abnf` indirectly through the
  canonical valid and invalid corpus already used by `toml_dom`.
- LSP integration tests communicate with the server over in-memory streams and
  assert diagnostics, completion and navigation results.
- VS Code tests cover activation, file association, server startup and a small
  end-to-end smoke test; language semantics remain in Rust tests.
- Each feature must include tests for Unicode because LSP columns are UTF-16
  code units while Rust and `toml_dom` operate on UTF-8 byte offsets.
- A deterministic generated UTF-8 corpus guards the complete analysis entry
  point against panics, including Unicode scalar and boundary cases.
- Coverage-guided fuzz targets exercise analysis and all editor-facing core
  features; short CI runs complement longer local campaigns.
- A 10,000-line generated document records the large-file analysis time and
  enforces a generous CI safety ceiling.
- English and German VS Code manifest catalogs must contain exactly the same
  localization keys; the TextMate test suite checks that invariant.

## Initial decisions

- Language-server transport: standard input/output.
- Document synchronization: incremental LSP changes, normalized into a complete
  versioned snapshot before parsing.
- Parsing baseline: `toml_dom = "0.4"`; no external TOML parser and no
  Tree-sitter dependency.
- Syntax highlighting: TextMate grammar first, semantic tokens later.
- Error recovery: parse the complete snapshot first; recover independent Bare
  Path Reference errors with position-stable placeholders and retain the last
  valid line prefix while a later region is syntactically incomplete.
- Release model: publish the VS Code extension with platform-specific Rust
  server binaries so end users do not need a Rust toolchain.

## First implementation milestone

Stage 1 is complete. It includes debounced, version-safe incremental document
synchronization, parsing and syntax diagnostics, hierarchical document symbols,
server logging, VS Code language registration, and tested TextMate highlighting.
During a development run, the extension starts the server through Cargo;
release builds will place a native binary in `vscode-atml/bin/`.

Stage 2 is complete as well. Every valid document snapshot now carries an
editor-neutral semantic index containing key and table definitions, value
types, enum definitions and choices, enum and path references, quantities,
inheritance edges, direct and transitive targets, and graph cycles. The index
uses UTF-8 source ranges and stable definition IDs and is cached until the next
document version arrives.

Stage 3 adds multi-error semantic diagnostics. Unknown and cyclic path
references, enum binding and ordering errors, missing or invalid inheritance
parents, and inheritance cycles are reported with stable codes and exact source
ranges. Syntax, TOML semantics, and ATML semantics remain separate categories.

Stage 4 adds context-sensitive completion implemented entirely in the Rust
core and exposed through LSP. It completes visible enum members and enum names,
Bare Path References, standard inheritance parents, basic TOML/ATML value
shapes, and units already used earlier in the document. Suggestions use the
authored prefix, replace only the relevant UTF-8/UTF-16 range, prefer local
symbols, and remain available while the current line is still incomplete.

Stage 5 adds semantic hover and navigation. Hover describes key types,
quantity components, enum choices, direct and transitively resolved path
targets, and inherited values with their original parent tables. Go-to-
definition follows enum members, the next authored link in a path-reference
chain, and inheritance parents. Find References covers enum definitions, keys,
tables, direct uses, and transitively resolved uses. All targets use exact name
ranges; the LSP layer only converts UTF-8 byte offsets to UTF-16 positions.

Stage 6 makes editing robust. A valid document prefix remains semantically
available while the current line is incomplete. Semantic Tokens distinguish
ATML symbols beyond TextMate's lexical context. Prepare Rename and Rename update
bare key, enum, and table definitions plus every directly affected authored
use, rejecting invalid names, conflicts, and unsafe quoted rewrites. Quick Fixes
are limited to unique case corrections for existing enums, members, paths, and
inheritance parents. The separate [formatting contract](FORMATTING.md) defines
the required DST/CST invariants; formatting is not advertised until those
invariants have an implementation and fixture coverage.

Stage 7 release preparation produces six target-specific packages for Linux,
Windows, and macOS on x64 and ARM64. The TypeScript client and its runtime
dependencies are bundled into one JavaScript file; each VSIX contains exactly
one native Rust server. Rust, VSCE, esbuild, and both dependency lockfiles are
pinned. Packaging normalizes ZIP order and timestamps, verifies an allowlisted
content shape, and emits SHA-256 checksums. The CI release matrix and protected
publication procedure are documented in [RELEASING.md](RELEASING.md).

| Diagnostic code | Meaning |
|---|---|
| `atml.syntax.parse-error` | malformed TOML or ATML syntax |
| `toml.duplicate-key` | duplicate TOML key or conflicting table definition |
| `toml.integer-overflow` | integer outside the supported TOML range |
| `toml.invalid-escape` | invalid TOML string escape |
| `atml.reference.unknown-target` | missing Bare Path Reference target |
| `atml.reference.cycle` | cyclic Bare Path Reference chain |
| `atml.enum.unknown-definition` | reference to an unknown enum |
| `atml.enum.unknown-member` | symbol absent from the referenced enum |
| `atml.enum.used-before-definition` | enum referenced before its declaration |
| `atml.inheritance.unknown-parent` | missing inherited table |
| `atml.inheritance.invalid-parent-type` | parent is not a standard table |
| `atml.inheritance.cycle` | cyclic table inheritance |

From this directory, verify the Rust workspace with:

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```

Verify the VS Code client with:

```sh
cd vscode-atml
npm install
npm run check
npm run compile
npm run test:grammar
```

Build a local target-specific release after compiling the native server:

```sh
cd vscode-atml
./scripts/package.sh linux-x64 ../target/release/atml-language-server
```

The script bundles the client, creates and verifies the VSIX, normalizes it for
reproducibility, and writes a neighboring `.sha256` file. Marketplace
publication is intentionally separate and requires the protected publisher
credential described in `RELEASING.md`.

Open `vscode-atml` in VS Code and press F5 to start an Extension Development
Host. Opening an `.atml` file there starts the Rust server automatically.
The full VS Code smoke test requires Node.js 22 and can be run with
`npm run test:vscode`; Linux CI executes it under Xvfb.

The first milestone is complete when a locally installed development extension
opens an `.atml` file, starts the Rust server, parses it with `toml_dom 0.4`,
highlights TOML and ATML syntax, and updates a syntax diagnostic after an edit
without restarting the server.
