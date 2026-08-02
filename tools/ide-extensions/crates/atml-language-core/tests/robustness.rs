use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

use atml_language_core::analyze;

#[test]
fn analyzes_a_10_000_line_document_within_the_interactive_budget() {
    let mut source = String::with_capacity(220_000);
    for index in 0..10_000 {
        source.push_str(&format!("key_{index} = {index}\n"));
    }

    let started = Instant::now();
    let analysis = analyze(&source);
    let elapsed = started.elapsed();

    assert!(
        analysis.diagnostics.is_empty(),
        "large valid fixture produced diagnostics"
    );
    assert_eq!(analysis.symbols.len(), 10_000);
    assert!(
        elapsed < Duration::from_secs(30),
        "10,000-line analysis exceeded the 30-second CI safety budget: {elapsed:?}"
    );
    eprintln!("10,000-line ATML analysis: {elapsed:?}");
}

#[test]
fn arbitrary_utf8_input_never_panics() {
    const CASES: usize = 4_096;
    let mut state = 0x4154_4d4c_d15e_a5e5_u64;

    for case in 0..CASES {
        let length = (next(&mut state) % 257) as usize;
        let mut source = String::with_capacity(length * 2);
        for _ in 0..length {
            let scalar = (next(&mut state) % 0x11_0000) as u32;
            source.push(char::from_u32(scalar).unwrap_or('\u{fffd}'));
        }

        let result = catch_unwind(AssertUnwindSafe(|| analyze(&source)));
        assert!(
            result.is_ok(),
            "analysis panicked for deterministic UTF-8 case {case}"
        );
    }

    for source in [
        "",
        "\0",
        "\u{feff}",
        "😀🦀𐐷",
        "key = \"e\u{301}\"",
        "\r\n\r\n",
        "\u{10ffff}",
    ] {
        assert!(
            catch_unwind(AssertUnwindSafe(|| analyze(source))).is_ok(),
            "analysis panicked for curated UTF-8 boundary case"
        );
    }
}

#[test]
fn key_like_text_before_unicode_never_corrupts_source_ranges() {
    // Minimized from the first crash found by the GitHub fuzz smoke test.
    let source = concat!(
        "naddname = \"Kdddddäfer 🦀\"\n",
        "#nadna\u{1}.dmeme  = \"Käfer 🦀\"\n",
        "d = \"Kdddddäfer) 🦀\"\n",
        "#nadna\u{1}.dmeme  ",
    );

    assert!(catch_unwind(AssertUnwindSafe(|| analyze(source))).is_ok());
}

fn next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state
}
