use atml_language_core::{
    analyze, prepare_rename, quick_fixes, rename, semantic_tokens, ByteRange, RenameError,
    SemanticTokenKind, TextChange,
};

fn inside(source: &str, needle: &str) -> usize {
    source.find(needle).unwrap() + needle.len() / 2
}

fn apply(source: &str, changes: &[TextChange]) -> String {
    let mut result = source.to_owned();
    let mut changes = changes.to_vec();
    changes.sort_by_key(|change| std::cmp::Reverse(change.range.start));
    for change in changes {
        result.replace_range(change.range.start..change.range.end, &change.new_text);
    }
    result
}

#[test]
fn incomplete_last_line_preserves_the_valid_prefix_model() {
    let source = "Mode[] = [Active, Passive]\n[root]\nspeed = 5ms\nbroken = [\n";
    let analysis = analyze(source);
    assert!(analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "atml.syntax.parse-error"));
    let index = analysis.semantic.expect("tolerant semantic prefix");
    assert!(index
        .definitions
        .iter()
        .any(|definition| definition.path == ["root", "speed"]));
    assert!(semantic_tokens(source, &index)
        .iter()
        .any(|token| token.kind == SemanticTokenKind::Unit));
}

#[test]
fn semantic_tokens_distinguish_atml_constructs_without_overlaps() {
    let source = "Mode[] = [Active, Passive]\n[root]\nspeed = 5m²\n[child : root]\nmode = Mode::Active\ncopy = root.speed\n";
    let index = analyze(source).semantic.unwrap();
    let tokens = semantic_tokens(source, &index);
    for expected in [
        SemanticTokenKind::Property,
        SemanticTokenKind::Table,
        SemanticTokenKind::Enum,
        SemanticTokenKind::EnumMember,
        SemanticTokenKind::Reference,
        SemanticTokenKind::Number,
        SemanticTokenKind::Unit,
    ] {
        assert!(tokens.iter().any(|token| token.kind == expected));
    }
    assert!(tokens
        .windows(2)
        .all(|pair| pair[0].range.end <= pair[1].range.start));
}

#[test]
fn safely_renames_keys_enums_and_tables_with_all_authored_uses() {
    let source = "Mode[] = [Active, Passive]\n[root]\nspeed = 5ms\n[root.child]\ncopy = root.speed\n[derived : root]\nmode = Mode::Active\n";
    let index = analyze(source).semantic.unwrap();

    let key_changes = rename(source, &index, inside(source, "speed ="), "velocity").unwrap();
    let key_result = apply(source, &key_changes);
    assert!(key_result.contains("velocity = 5ms"));
    assert!(key_result.contains("root.velocity"));
    assert!(analyze(&key_result).diagnostics.is_empty());
    let key_use = source.find("root.speed").unwrap() + "root.".len();
    let prepared_key = prepare_rename(source, &index, key_use).unwrap();
    assert_eq!(
        &source[prepared_key.range.start..prepared_key.range.end],
        "speed"
    );

    let enum_changes = rename(source, &index, inside(source, "Mode[]"), "Strategy").unwrap();
    let enum_result = apply(source, &enum_changes);
    assert!(enum_result.contains("Strategy[]"));
    assert!(enum_result.contains("Strategy::Active"));
    assert!(analyze(&enum_result).diagnostics.is_empty());
    let enum_use = source.rfind("Mode::Active").unwrap() + 1;
    let enum_from_use = rename(source, &index, enum_use, "Strategy").unwrap();
    assert_eq!(apply(source, &enum_from_use), enum_result);

    let table_changes = rename(source, &index, inside(source, "[root]"), "base").unwrap();
    let table_result = apply(source, &table_changes);
    assert!(table_result.contains("[base]"));
    assert!(table_result.contains("[base.child]"));
    assert!(table_result.contains("base.speed"));
    assert!(table_result.contains("[derived : base]"));
    assert!(analyze(&table_result).diagnostics.is_empty());
    let table_use = source.find("root.speed").unwrap() + 1;
    let table_from_use = rename(source, &index, table_use, "base").unwrap();
    assert_eq!(apply(source, &table_from_use), table_result);
}

#[test]
fn rename_rejects_invalid_names_conflicts_and_non_symbols() {
    let source = "[root]\nfirst = 1\nsecond = 2\n";
    let index = analyze(source).semantic.unwrap();
    assert_eq!(
        rename(source, &index, inside(source, "first"), "second"),
        Err(RenameError::Conflict)
    );
    assert_eq!(
        rename(source, &index, inside(source, "first"), "bad.name"),
        Err(RenameError::InvalidName)
    );
    assert!(prepare_rename(source, &index, source.find('1').unwrap()).is_none());
}

#[test]
fn quick_fixes_only_correct_unique_case_mismatches() {
    let source = "Mode[] = [Active]\n[root]\nspeed = 1\na = Mode::active\nb = mode::Active\nc = Root.speed\n[child : Root]\nmissing = absent.value\n";
    let analysis = analyze(source);
    let index = analysis.semantic.as_ref().unwrap();
    let fixes = quick_fixes(
        source,
        index,
        &analysis.diagnostics,
        ByteRange {
            start: 0,
            end: source.len(),
        },
    );
    assert_eq!(fixes.len(), 4);
    let fixed = apply(
        source,
        &fixes
            .iter()
            .map(|fix| fix.change.clone())
            .collect::<Vec<_>>(),
    );
    assert!(fixed.contains("Mode::Active"));
    assert!(fixed.contains("root.speed"));
    assert!(fixed.contains("[child : root]"));
    assert!(fixed.contains("absent.value"));
}
