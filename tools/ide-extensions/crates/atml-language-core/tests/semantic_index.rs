use atml_language_core::{analyze, CycleKind, DefinitionKind, ReferenceKind, ValueType};

fn path(segments: &[&str]) -> Vec<String> {
    segments
        .iter()
        .map(|segment| (*segment).to_owned())
        .collect()
}

#[test]
fn indexes_all_atml_constructs_and_source_ranges() {
    let source = "Mode[] = [Active, Passive]\n\
                  [defaults]\n\
                  timeout = 500ms\n\
                  [server : defaults]\n\
                  mode = Mode::Active\n\
                  copied = defaults.timeout\n\
                  rate = 1.80EUR/L\n\
                  products = [5N*m, 2m²]\n";
    let analysis = analyze(source);
    assert!(analysis.diagnostics.is_empty());
    let index = analysis.semantic.expect("semantic index");

    let mode = index
        .definitions_at_path(&path(&["Mode"]))
        .next()
        .expect("enum definition");
    assert_eq!(mode.kind, DefinitionKind::Enum);
    assert_eq!(mode.value_type, ValueType::EnumDefinition);
    assert_eq!(
        &source[mode.selection_range.start..mode.selection_range.end],
        "Mode[]"
    );

    let active = index
        .definitions_at_path(&path(&["Mode", "Active"]))
        .next()
        .expect("enum member");
    assert_eq!(active.kind, DefinitionKind::EnumMember);
    assert_eq!(&source[active.range.start..active.range.end], "Active");

    let enum_use = index
        .references
        .iter()
        .find(|reference| reference.kind == ReferenceKind::EnumMember)
        .expect("enum reference");
    assert_eq!(enum_use.target, Some(active.id));
    assert_eq!(enum_use.resolved_target, Some(active.id));
    assert_eq!(
        &source[enum_use.range.start..enum_use.range.end],
        "Mode::Active"
    );

    let path_use = index
        .references
        .iter()
        .find(|reference| reference.kind == ReferenceKind::Path)
        .expect("path reference");
    let timeout = index
        .definitions_at_path(&path(&["defaults", "timeout"]))
        .next()
        .expect("path target");
    assert_eq!(path_use.target, Some(timeout.id));
    assert_eq!(path_use.resolved_target, Some(timeout.id));
    assert_eq!(
        &source[path_use.range.start..path_use.range.end],
        "defaults.timeout"
    );

    assert_eq!(index.inheritance.len(), 1);
    let defaults = index
        .definitions_at_path(&path(&["defaults"]))
        .find(|definition| definition.kind == DefinitionKind::Table)
        .expect("parent table");
    assert_eq!(index.inheritance[0].parent, Some(defaults.id));
    assert_eq!(index.inheritance[0].child_path, path(&["server"]));

    let authored_quantities = index
        .quantities
        .iter()
        .map(|quantity| &source[quantity.range.start..quantity.range.end])
        .collect::<Vec<_>>();
    assert_eq!(authored_quantities, ["500ms", "1.80EUR/L", "5N*m", "2m²"]);
}

#[test]
fn resolves_path_references_transitively() {
    let source = "[root]\n\
                  value = 5ms\n\
                  [refs]\n\
                  first = root.value\n\
                  second = refs.first\n";
    let index = analyze(source).semantic.expect("semantic index");
    let value = index
        .definitions_at_path(&path(&["root", "value"]))
        .next()
        .unwrap();
    let second = index
        .references
        .iter()
        .find(|reference| reference.source_path == path(&["refs", "second"]))
        .unwrap();
    assert_ne!(second.target, Some(value.id));
    assert_eq!(second.resolved_target, Some(value.id));
}

#[test]
fn indexes_symbolic_and_ordinary_enum_choices() {
    let source = "Ports[] = [110, 143]\nMixed[] = [Automatic, \"manual\"]\n";
    let index = analyze(source).semantic.expect("semantic index");

    for expected in [
        path(&["Ports", "110"]),
        path(&["Ports", "143"]),
        path(&["Mixed", "Automatic"]),
        path(&["Mixed", "\"manual\""]),
    ] {
        assert!(index
            .definitions
            .iter()
            .any(|definition| definition.path == expected
                && definition.kind == DefinitionKind::EnumMember));
    }
}

#[test]
fn detects_inheritance_cycles_in_a_document() {
    let source = "[safe]\nvalue = 1\n[a : safe, b]\n[b : a]\n";
    let index = analyze(source).semantic.expect("semantic index");
    assert_eq!(index.cycles.len(), 1);
    assert_eq!(index.cycles[0].kind, CycleKind::Inheritance);
    let paths = &index.cycles[0].paths;
    assert_eq!(paths.first(), paths.last());
    assert!(paths.contains(&path(&["a"])));
    assert!(paths.contains(&path(&["b"])));
}

#[test]
fn indexes_vehicle_rental_example() {
    let source = include_str!("../../../../../examples/vehicle-rental.atml");
    let analysis = analyze(source);
    assert!(analysis.diagnostics.is_empty());
    let index = analysis.semantic.expect("semantic index");

    assert!(index
        .definitions
        .iter()
        .any(|definition| definition.path == path(&["DriveType"])
            && definition.kind == DefinitionKind::Enum));
    assert!(index
        .definitions
        .iter()
        .any(|definition| definition.path == path(&["fleet"])
            && definition.kind == DefinitionKind::ArrayOfTables));
    assert!(index.references.len() > 40);
    assert!(index.quantities.len() > 40);
    assert!(index.inheritance.len() > 30);
    assert!(index
        .references
        .iter()
        .all(|reference| reference.target.is_some()));
    assert!(index.inheritance.iter().all(|edge| edge.parent.is_some()));
    assert!(index.cycles.is_empty());

    let compact_parents = index
        .inheritance_chain(&path(&["class", "car", "compact"]))
        .into_iter()
        .map(|definition| definition.path.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        compact_parents,
        [path(&["class", "car"]), path(&["vehicle"])]
    );
}

#[test]
fn indexes_every_official_example_with_valid_ranges() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../examples");
    let mut checked = 0;
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
        let index = analysis.semantic.unwrap();
        for definition in &index.definitions {
            assert!(definition.range.end <= source.len());
            assert!(source.is_char_boundary(definition.range.start));
            assert!(source.is_char_boundary(definition.range.end));
        }
        for reference in &index.references {
            assert!(reference.range.end <= source.len());
            assert!(source.is_char_boundary(reference.range.start));
            assert!(source.is_char_boundary(reference.range.end));
        }
        checked += 1;
    }
    assert!(checked > 0);
}
