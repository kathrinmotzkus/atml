# Fuzzing the ATML language tooling

Fuzzing complements the deterministic robustness and 10,000-line tests. It
uses mutation-guided coverage to explore malformed, incomplete, Unicode-heavy,
and nearly valid ATML documents that hand-written fixtures may not anticipate.

The `analyze` target exercises parsing, recovery, diagnostics, symbols, and the
semantic index. The `language_features` target additionally calls completion,
hover, navigation, references, Semantic Tokens, Quick Fixes, and Rename at
multiple source offsets.

Run a bounded local session from `tools/ide-extensions`:

```sh
ASAN_OPTIONS=detect_leaks=0 cargo +nightly-2026-05-25 fuzz run analyze -- -max_total_time=60 -max_len=65536
ASAN_OPTIONS=detect_leaks=0 cargo +nightly-2026-05-25 fuzz run language_features -- -max_total_time=60 -max_len=16384
```

Run without `-max_total_time` for a sustained session. Crashes are written
under `fuzz/artifacts/`; minimize them with `cargo fuzz tmin`, then add the
smallest reproducer as a normal regression test. Useful non-crashing inputs can
be retained as named seed files under the corresponding `fuzz/corpus/`
directory; transient hash-named mutations are ignored by Git.

The CI smoke job uses short time limits. It catches build regressions and
obvious crashes but does not replace longer local or scheduled campaigns.
LeakSanitizer is disabled because it is incompatible with some container and
process-monitoring environments; AddressSanitizer's other memory-safety checks
remain active.
