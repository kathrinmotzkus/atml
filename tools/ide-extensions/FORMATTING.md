# Format-preserving ATML formatting concept

Formatting is intentionally not exposed as an LSP capability yet. A conventional
pretty printer would destroy information that matters in ATML source files. The
formatter must operate on `toml_dom`'s document syntax tree (DST/CST) and satisfy
the invariants below before `textDocument/formatting` is advertised.

## Invariants

1. Parsing and serializing a document without requested changes is byte-for-byte
   identical.
2. Comments, blank lines, line endings, quote styles, numeric notation,
   underscores, trailing commas, inline-versus-block tables, enum spelling,
   Quantity exponents, Bare Path References, and inheritance headers remain
   unchanged unless the selected formatting rule directly targets them.
3. A formatting edit is the smallest source edit that implements one named
   rule. Unrelated document regions are never regenerated.
4. Invalid or only tolerantly recovered documents receive no formatting edits.
5. Formatting is idempotent and the result must parse with
   `Document::parse_atml` without changing the semantic index.

## Implementation boundary

The formatter will parse the complete valid snapshot with `toml_dom 0.4` and
work on `DocumentItem`, `EntryNode`, `ValueNode`, `SectionNode`, and their raw
formatting fields. Existing format-preserving operations such as `set_value`
may be used for intentional value changes, but ordinary formatting must not use
the canonical serializer, `sort_keys`, `prefer_inline`, or `root_mut()` as a
shortcut. Output is produced through `Document::serialize()`.

The first rule set is deliberately small:

- normalize whitespace immediately around `=` while retaining comments;
- normalize indentation of elements in multiline arrays and inline tables;
- optionally ensure one final newline, preserving the existing line-ending
  convention;
- never reorder keys, tables, parents, enum members, or array elements.

Each rule returns CST-local byte ranges. The language core compares the
serialized result with the original source and converts only changed spans into
editor-neutral text edits. The LSP server then converts their UTF-8 offsets to
UTF-16 positions.

## Required tests before implementation

- Byte-identical no-op round trips for every official example.
- One fixture per preserved spelling and comment/whitespace category.
- ATML fixtures for all Quantity exponent forms, references, enums, multiple
  inheritance parents, and arrays of inherited tables.
- Idempotence: formatting twice produces no second edit.
- Semantic equivalence before and after every edit.
- Unicode and CRLF range tests.
- Property tests ensuring arbitrary valid input never panics and invalid input
  never receives an edit.

Only after these tests and the CST-local rule implementation exist should the
server advertise document or range formatting.
