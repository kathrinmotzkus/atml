use std::collections::HashSet;

use atml_language_core::{analyze, DiagnosticSeverity};

fn codes(source: &str) -> HashSet<&'static str> {
    analyze(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn reports_multiple_independent_enum_and_inheritance_errors() {
    let source = "early = Later::Ready\n\
                  Mode[] = [Active]\n\
                  bad_member = Mode::Missing\n\
                  bad_enum = Missing::Value\n\
                  Later[] = [Ready]\n\
                  scalar = 1\n\
                  [child : absent, scalar]\n";
    let analysis = analyze(source);
    let actual = analysis
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<HashSet<_>>();

    assert_eq!(analysis.diagnostics.len(), 5);
    assert_eq!(
        actual,
        HashSet::from([
            "atml.enum.used-before-definition",
            "atml.enum.unknown-member",
            "atml.enum.unknown-definition",
            "atml.inheritance.unknown-parent",
            "atml.inheritance.invalid-parent-type",
        ])
    );
    assert!(analysis
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error));
}

#[test]
fn reports_multiple_unknown_path_references_after_recovery() {
    let source = "first = missing.one\nsecond = other.value\narray = [third.value,\n  fourth.value]\nvalid = 1\n";
    let analysis = analyze(source);
    assert!(analysis.semantic.is_some());
    assert_eq!(analysis.diagnostics.len(), 4);
    assert!(analysis
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.code == "atml.reference.unknown-target"));
    assert_eq!(
        analysis
            .diagnostics
            .iter()
            .map(|diagnostic| &source[diagnostic.range.start..diagnostic.range.end])
            .collect::<Vec<_>>(),
        ["missing.one", "other.value", "third.value", "fourth.value"]
    );
}

#[test]
fn reports_each_location_in_a_path_reference_cycle() {
    let source = "[refs]\na = refs.b\nb = refs.a\n";
    let analysis = analyze(source);
    assert!(analysis.semantic.is_some());
    assert_eq!(
        analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "atml.reference.cycle")
            .count(),
        2
    );
}

#[test]
fn reports_inheritance_cycles_and_precise_parent_ranges() {
    let source = "[a : b]\n[b : a]\n";
    let analysis = analyze(source);
    assert_eq!(codes(source), HashSet::from(["atml.inheritance.cycle"]));
    let authored = analysis
        .diagnostics
        .iter()
        .map(|diagnostic| &source[diagnostic.range.start..diagnostic.range.end])
        .collect::<HashSet<_>>();
    assert_eq!(authored, HashSet::from(["a", "b"]));
}

#[test]
fn rejects_array_of_tables_as_an_inheritance_parent() {
    let source = "[[templates]]\nname = \"base\"\n[[items : templates]]\nname = \"item\"\n";
    assert_eq!(
        codes(source),
        HashSet::from(["atml.inheritance.invalid-parent-type"])
    );
}

#[test]
fn accepts_an_implicit_standard_table_as_parent() {
    let source = "implicit.value = 1\n[child : implicit]\nown = 2\n";
    assert!(analyze(source).diagnostics.is_empty());
}

#[test]
fn official_examples_have_no_semantic_diagnostics() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../examples");
    for entry in std::fs::read_dir(examples).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("atml") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let analysis = analyze(&source);
        assert!(
            analysis.diagnostics.is_empty(),
            "{}: {:?}",
            path.display(),
            analysis.diagnostics
        );
    }
}

#[test]
fn distinguishes_toml_semantics_from_syntax_errors() {
    assert_eq!(
        codes("value = 1\nvalue = 2\n"),
        HashSet::from(["toml.duplicate-key"])
    );
    assert_eq!(
        codes("value = [\n"),
        HashSet::from(["atml.syntax.parse-error"])
    );
}
