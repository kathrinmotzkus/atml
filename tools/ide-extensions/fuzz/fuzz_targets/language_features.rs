#![no_main]

use atml_language_core::{
    analyze, complete, find_references, goto_definition, hover, prepare_rename, quick_fixes,
    rename, semantic_tokens, ByteRange,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    let analysis = analyze(source);
    let Some(index) = analysis.semantic.as_ref() else {
        return;
    };

    let mut offsets = vec![0, source.len() / 2, source.len()];
    offsets.extend(source.char_indices().take(13).map(|(offset, _)| offset));
    offsets.sort_unstable();
    offsets.dedup();

    let _ = semantic_tokens(source, index);
    let _ = quick_fixes(
        source,
        index,
        &analysis.diagnostics,
        ByteRange {
            start: 0,
            end: source.len(),
        },
    );
    for offset in offsets {
        let _ = complete(source, offset);
        let _ = hover(source, index, offset);
        let _ = goto_definition(index, offset);
        let _ = find_references(index, offset, true);
        let _ = prepare_rename(source, index, offset);
        let _ = rename(source, index, offset, "fuzz_name");
    }
});
