# ATML Tools for Visual Studio Code

Language support for [ATML (Advanced TOML)](https://github.com/kathrinmotzkus/atml),
an additive extension of TOML 1.1 with quantities, Bare Path References, enums,
and table inheritance.

## Features

- TOML 1.1 and ATML syntax highlighting
- syntax and semantic diagnostics with precise source ranges
- context-aware completion for enums, paths, inheritance parents, values, and units
- hover information for values, quantities, references, and inherited values
- document symbols, go to definition, and find references
- Semantic Tokens, safe rename, and conservative Quick Fixes
- robust language intelligence while the current line is incomplete

The extension uses the `toml_dom 0.4` ATML parser through a native Rust language
server. There is no second TOML parser and no Tree-sitter dependency.

## Installation

Install **ATML Tools** from the Visual Studio Marketplace or install
the VSIX matching your platform:

| VSIX target | Operating system | Architecture |
|---|---|---|
| `linux-x64` | Linux | x86-64 |
| `linux-arm64` | Linux | ARM64 |
| `win32-x64` | Windows | x86-64 |
| `win32-arm64` | Windows | ARM64 |
| `darwin-x64` | macOS | Intel |
| `darwin-arm64` | macOS | Apple Silicon |

Published packages contain the matching native language server. End users do
not need Rust, Cargo, Node.js, or npm.

## Configuration

- `atml.server.path`: optional absolute path to a custom language-server binary.
- `atml.trace.server`: `off`, `messages`, or `verbose` LSP tracing.
- Command **ATML: Restart Language Server** restarts the active server.

## Troubleshooting

1. Open **View → Output → ATML Language Server** and inspect the server log.
2. Run **ATML: Restart Language Server** after changing `atml.server.path`.
3. Confirm that the installed VSIX target matches the operating system and CPU.
4. Remove a custom `atml.server.path` to return to the bundled server.
5. Report reproducible problems with the ATML source and extension version at
   [GitHub Issues](https://github.com/kathrinmotzkus/atml/issues).

The extension does not yet advertise document formatting. Its format-preserving
contract is documented in the source repository before an implementation is enabled.

## Privacy

ATML documents are parsed locally. The extension does not transmit document
contents or include telemetry of its own.

## License

MIT. See `LICENSE` in the extension package.
