use atml_language_core::{complete, CompletionKind};

fn at_cursor(marked: &str) -> (String, usize) {
    let offset = marked.find('|').expect("cursor marker");
    (marked.replacen('|', "", 1), offset)
}

fn labels(marked: &str) -> Vec<String> {
    let (source, offset) = at_cursor(marked);
    complete(&source, offset)
        .into_iter()
        .map(|item| item.label)
        .collect()
}

#[test]
fn completes_visible_enum_members_and_replaces_only_the_member_prefix() {
    let (source, offset) = at_cursor("Strategy[] = [Active, Passive]\nchoice = Strategy::A|\n");
    let items = complete(&source, offset);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "Active");
    assert_eq!(items[0].kind, CompletionKind::EnumMember);
    assert_eq!(&source[items[0].replace.start..items[0].replace.end], "A");
    assert_eq!(items[0].insert_text, "Active");

    assert!(labels("choice = Future::|\nFuture[] = [One]\n").is_empty());
}

#[test]
fn completes_reference_paths_and_sorts_local_symbols_first() {
    let suggestions =
        labels("[defaults]\ntimeout = 500ms\n[server]\nhost = \"localhost\"\ncopy = defaults.t|\n");
    assert_eq!(suggestions, ["defaults.timeout"]);

    let (source, offset) =
        at_cursor("[defaults]\ntimeout = 500ms\n[server]\nhost = \"localhost\"\ncopy = |\n");
    let items = complete(&source, offset);
    let local = items
        .iter()
        .position(|item| item.label == "server.host")
        .unwrap();
    let remote = items
        .iter()
        .position(|item| item.label == "defaults.timeout")
        .unwrap();
    assert!(local < remote);
}

#[test]
fn completes_only_standard_tables_as_inheritance_parents() {
    let suggestions = labels("[defaults]\nvalue = 1\n[[fleet]]\nname = \"car\"\n[child : def|\n");
    assert_eq!(suggestions, ["defaults"]);
}

#[test]
fn completes_visible_enums_and_value_shapes_at_empty_values() {
    let (source, offset) =
        at_cursor("Strategy[] = [Active, Passive]\n[server]\nchoice = |\nFuture[] = [One]\n");
    let items = complete(&source, offset);
    let strategy = items.iter().find(|item| item.label == "Strategy").unwrap();
    assert_eq!(strategy.kind, CompletionKind::Enum);
    assert_eq!(strategy.insert_text, "Strategy::");
    assert!(!items.iter().any(|item| item.label == "Future"));
    for value in ["\"\"", "0", "0.0", "true", "false", "[]", "{}", "0unit"] {
        assert!(
            items.iter().any(|item| item.label == value),
            "missing {value}"
        );
    }
}

#[test]
fn completes_known_quantity_units_including_units_beginning_with_e() {
    let suggestions = labels("short = 500ms\nprice = 1.80EUR/L\nnext = 10m|\n");
    assert_eq!(suggestions, ["ms"]);
    let suggestions = labels("price = 1.80EUR\nnext = 1.5E|\n");
    assert_eq!(suggestions, ["EUR"]);
}

#[test]
fn avoids_completion_in_keys_comments_strings_and_invalid_value_contexts() {
    assert!(labels("na|me = 1\n").is_empty());
    assert!(labels("value = 1 # Strategy::|\n").is_empty());
    assert!(labels("value = \"Strategy::|\"\n").is_empty());
    assert!(labels("value = [1, |]\n").is_empty());
}

#[test]
fn completion_ranges_remain_utf8_byte_ranges() {
    let (source, offset) = at_cursor("title = \"Größe\"\nMode[] = [On, Off]\nvalue = Mode::O|\n");
    let items = complete(&source, offset);
    assert_eq!(&source[items[0].replace.start..items[0].replace.end], "O");
}
