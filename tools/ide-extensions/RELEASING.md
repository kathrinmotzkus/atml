# Releasing the ATML VS Code extension

The release consists of six target-specific VSIX files. Each contains exactly
one native language server and is selected by the Visual Studio Marketplace for
the matching operating system and architecture.

## Supported targets

| VSCE target | Rust target | CI runner |
|---|---|---|
| `linux-x64` | `x86_64-unknown-linux-gnu` | `ubuntu-22.04` |
| `linux-arm64` | `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` |
| `win32-x64` | `x86_64-pc-windows-msvc` | `windows-2022` |
| `win32-arm64` | `aarch64-pc-windows-msvc` | `windows-11-arm` |
| `darwin-x64` | `x86_64-apple-darwin` | `macos-15-intel` |
| `darwin-arm64` | `aarch64-apple-darwin` | `macos-15` |

Rust is pinned by `rust-toolchain.toml`, Node.js 22 is selected in CI, npm
dependencies are locked, Cargo uses `--locked`, and VSCE is pinned exactly.
The packaging script normalizes ZIP entry order and timestamps to the source
commit time before creating a SHA-256 file.

## Automated artifact build

Push a version tag or manually run the `CI` workflow. The
`ide-release-artifacts` matrix builds all six native binaries, creates the
targeted VSIX files, rejects unexpected source/cache/secret-like paths, and
uploads each VSIX with its checksum as a workflow artifact.

Before tagging:

1. Keep the extension and changelog versions synchronized.
2. Run every test from `tools/ide-extensions/README.md`.
3. Confirm that the worktree is clean and review `npm audit` and `cargo tree`.
4. Build and internally install the local platform VSIX.
5. Commit locally, review, then push through the normal repository workflow.

## Local Linux x64 package

```sh
cd tools/ide-extensions
cargo build --locked --release -p atml-language-server
cd vscode-atml
npm ci
./scripts/package.sh linux-x64 ../target/release/atml-language-server
code --install-extension dist/atml-0.1.0-linux-x64.vsix --force
```

Compare the `.sha256` file after transfer and open a fresh Extension
Development Host or VS Code profile with an `.atml` fixture. Diagnostics,
completion, hover, definition, references, Semantic Tokens, rename, and Quick
Fixes are the release smoke-test surface.

## Marketplace publication

Publication requires a Visual Studio Marketplace publisher named
`kathrinmotzkus` and a short-lived Azure DevOps Personal Access Token with only
Marketplace **Manage** permission. Supply it as `VSCE_PAT` in the publishing
environment; never place it in the repository, npm configuration, VSIX, logs,
or shell history.

After verifying all six checksums and smoke tests, publish each targeted package:

```sh
npx --no-install vsce publish --packagePath dist/atml-0.1.0-linux-x64.vsix
```

Repeat for all targets. Confirm the Marketplace target list and install the
public package once on a clean machine before creating the GitHub release.
Publication is deliberately not part of the ordinary CI job: it requires an
explicit authorized release decision and protected secret environment.

## Rollback

Do not overwrite a published version. If a release is faulty, unpublish only
when necessary, fix the source, increment the patch version, rebuild all six
artifacts, and retain the old checksums and workflow run for auditability.
