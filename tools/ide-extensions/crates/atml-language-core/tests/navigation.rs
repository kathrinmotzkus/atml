use atml_language_core::{
    analyze, find_references, goto_definition, hover, ByteRange, SemanticIndex,
};

const SOURCE: &str = "Mode[] = [Active, Passive]\n\
                     [root]\n\
                     speed = 5m²\n\
                     alias = root.speed\n\
                     [refs]\n\
                     first = root.alias\n\
                     second = refs.first\n\
                     [child : root]\n\
                     mode = Mode::Active\n";

fn index() -> SemanticIndex {
    let analysis = analyze(SOURCE);
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    analysis.semantic.unwrap()
}

fn inside(needle: &str) -> usize {
    SOURCE.find(needle).unwrap() + needle.len() / 2
}

fn authored(range: ByteRange) -> &'static str {
    &SOURCE[range.start..range.end]
}

#[test]
fn hovers_keys_quantities_enums_and_transitive_paths() {
    let index = index();

    let key = hover(SOURCE, &index, inside("speed =")).unwrap();
    assert_eq!(authored(key.range), "speed");
    assert!(key.markdown.contains("**Key** `root.speed`"));
    assert!(key.markdown.contains("Type: `quantity`"));
    assert!(key.markdown.contains("Definition: line 3, column 1"));

    let quantity = hover(SOURCE, &index, inside("5m²")).unwrap();
    assert_eq!(authored(quantity.range), "5m²");
    assert!(quantity.markdown.contains("Magnitude: `5`"));
    assert!(quantity.markdown.contains("Unit: `m`"));
    assert!(quantity.markdown.contains("Exponent: `²`"));
    assert!(quantity.markdown.contains("Super-unit: none"));

    let enum_value = hover(SOURCE, &index, inside("Mode::Active")).unwrap();
    assert!(enum_value.markdown.contains("Defined by: `Mode`"));
    assert!(enum_value.markdown.contains("Allowed: `Active`, `Passive`"));

    let path = hover(SOURCE, &index, inside("refs.first")).unwrap();
    assert!(path.markdown.contains("Direct target: `refs.first`"));
    assert!(path.markdown.contains("Resolved target: `root.speed`"));
    assert!(path.markdown.contains("Resolved value: `5m²`"));
}

#[test]
fn quantity_hover_includes_rate_super_unit_and_its_exponent() {
    let source = "rate = 1.80EUR/L^2\n";
    let index = analyze(source).semantic.unwrap();
    let result = hover(source, &index, source.find("EUR").unwrap()).unwrap();
    assert!(result.markdown.contains("Magnitude: `1.8`"));
    assert!(result.markdown.contains("Unit: `EUR`"));
    assert!(result
        .markdown
        .contains("Super-unit: `L` (per, exponent `^2`)"));
}

#[test]
fn namespaced_enum_hover_uses_atml_path_syntax() {
    let source = "catalog.Mode[] = [Active]\nvalue = catalog.Mode::Active\n";
    let index = analyze(source).semantic.unwrap();
    let result = hover(source, &index, source.rfind("Active").unwrap()).unwrap();
    assert!(result.markdown.contains("`catalog.Mode::Active`"));
    assert!(!result.markdown.contains("catalog::Mode::Active"));
}

#[test]
fn hover_describes_inherited_values_and_their_original_table() {
    let index = index();
    let parent = SOURCE.rfind("root]").unwrap() + 1;
    let result = hover(SOURCE, &index, parent).unwrap();
    assert_eq!(authored(result.range), "root");
    assert!(result.markdown.contains("**Inherited table** `root`"));
    assert!(result.markdown.contains("`speed` = `5m²` from `root`"));
}

#[test]
fn go_to_definition_uses_exact_authored_name_ranges() {
    let index = index();
    let enum_member = goto_definition(&index, inside("Mode::Active")).unwrap();
    assert_eq!(authored(enum_member.selection_range), "Active");

    let path = goto_definition(&index, inside("root.speed")).unwrap();
    assert_eq!(authored(path.selection_range), "speed");

    let parent = SOURCE.rfind("root]").unwrap() + 1;
    let table = goto_definition(&index, parent).unwrap();
    assert_eq!(authored(table.selection_range), "root");
}

#[test]
fn implicit_table_definitions_have_exact_navigation_ranges() {
    let source = "defaults.timeout = 5ms\n[child : defaults]\n";
    let index = analyze(source).semantic.unwrap();
    let use_offset = source.rfind("defaults").unwrap() + 1;
    let target = goto_definition(&index, use_offset).unwrap();
    assert_eq!(
        &source[target.selection_range.start..target.selection_range.end],
        "defaults"
    );
}

#[test]
fn finds_direct_and_transitively_resolved_references() {
    let index = index();
    let speed = find_references(&index, inside("speed ="), true);
    let authored_ranges = speed.into_iter().map(authored).collect::<Vec<_>>();
    assert_eq!(
        authored_ranges,
        ["speed", "root.speed", "root.alias", "refs.first"]
    );

    let enum_references = find_references(&index, inside("Mode[]"), false);
    assert_eq!(
        enum_references
            .into_iter()
            .map(authored)
            .collect::<Vec<_>>(),
        ["Mode::Active"]
    );

    let table_references = find_references(&index, inside("[root]"), false);
    assert_eq!(
        table_references
            .into_iter()
            .map(authored)
            .collect::<Vec<_>>(),
        ["root.speed", "root.alias", "root"]
    );
}

#[test]
fn returns_nothing_in_comments_and_plain_values() {
    let source = "value = 1 # root.speed\n";
    let index = analyze(source).semantic.unwrap();
    assert!(hover(source, &index, source.find("root").unwrap()).is_none());
    assert!(goto_definition(&index, source.find('1').unwrap()).is_none());
    assert!(find_references(&index, source.find('1').unwrap(), true).is_empty());
}
