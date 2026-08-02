use std::collections::HashSet;

use crate::{analyze, ByteRange, DefinitionKind, SemanticIndex, ValueType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompletionKind {
    Enum,
    EnumMember,
    Key,
    Table,
    Unit,
    Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: String,
    pub insert_text: String,
    pub replace: ByteRange,
    pub sort_text: String,
}

/// Produce context-sensitive completions at a UTF-8 byte offset.
pub fn complete(source: &str, offset: usize) -> Vec<CompletionItem> {
    let offset = floor_char_boundary(source, offset.min(source.len()));
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line = &source[line_start..offset];
    if in_comment_or_string(line) {
        return Vec::new();
    }

    let prefix_analysis = analyze(&source[..line_start]);
    let fallback_analysis;
    let index = if let Some(index) = prefix_analysis.semantic.as_ref() {
        index
    } else {
        fallback_analysis = analyze(source);
        let Some(index) = fallback_analysis.semantic.as_ref() else {
            return Vec::new();
        };
        index
    };

    if let Some(items) = complete_enum_members(line, line_start, offset, index) {
        return items;
    }
    if let Some(items) = complete_parent_tables(line, line_start, offset, index) {
        return items;
    }
    if let Some(items) = complete_units(line, line_start, offset, index) {
        return items;
    }
    complete_values_and_paths(line, line_start, offset, index)
}

fn complete_enum_members(
    line: &str,
    line_start: usize,
    offset: usize,
    index: &SemanticIndex,
) -> Option<Vec<CompletionItem>> {
    let separator = line.rfind("::")?;
    let enum_start = line[..separator]
        .rfind(|ch: char| matches!(ch, '=' | '[' | ',' | '{') || ch.is_whitespace())
        .map_or(0, |position| position + 1);
    let enum_name = line[enum_start..separator].trim();
    if enum_name.is_empty() {
        return Some(Vec::new());
    }
    let enum_path = enum_name
        .split('.')
        .map(|segment| segment.trim_matches(['"', '\'']).to_owned())
        .collect::<Vec<_>>();
    let typed = &line[separator + 2..];
    if !typed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Some(Vec::new());
    }
    let replace = ByteRange {
        start: line_start + separator + 2,
        end: offset,
    };
    let mut items = index
        .definitions
        .iter()
        .filter(|definition| {
            definition.kind == DefinitionKind::EnumMember
                && definition.value_type == ValueType::EnumReference
                && definition.path.len() == enum_path.len() + 1
                && definition.path.starts_with(&enum_path)
        })
        .filter_map(|definition| {
            let label = definition.path.last()?.clone();
            label.starts_with(typed).then(|| CompletionItem {
                label: label.clone(),
                kind: CompletionKind::EnumMember,
                detail: format!("member of enum {enum_name}"),
                insert_text: label,
                replace,
                sort_text: "0".into(),
            })
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| left.label.cmp(&right.label));
    Some(items)
}

fn complete_parent_tables(
    line: &str,
    line_start: usize,
    offset: usize,
    index: &SemanticIndex,
) -> Option<Vec<CompletionItem>> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('[') || trimmed.contains(']') {
        return None;
    }
    let colon = line.find(':')?;
    let segment_start = line[colon + 1..]
        .rfind(',')
        .map_or(colon + 1, |position| colon + 2 + position);
    let leading = line[segment_start..].len() - line[segment_start..].trim_start().len();
    let typed_start = segment_start + leading;
    let typed = line[typed_start..].trim_end();
    if !is_path_prefix(typed) {
        return Some(Vec::new());
    }
    let replace = ByteRange {
        start: line_start + typed_start,
        end: offset,
    };
    let mut seen = HashSet::new();
    let mut items = index
        .definitions
        .iter()
        .filter(|definition| definition.kind == DefinitionKind::Table)
        .filter_map(|definition| {
            let label = definition.path.join(".");
            (label.starts_with(typed) && seen.insert(label.clone())).then(|| CompletionItem {
                label: label.clone(),
                kind: CompletionKind::Table,
                detail: "ATML parent table".into(),
                insert_text: label,
                replace,
                sort_text: "0".into(),
            })
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| left.label.cmp(&right.label));
    Some(items)
}

fn complete_units(
    line: &str,
    line_start: usize,
    offset: usize,
    index: &SemanticIndex,
) -> Option<Vec<CompletionItem>> {
    let equals = line.rfind('=')?;
    let token_start =
        equals + 1 + (line[equals + 1..].len() - line[equals + 1..].trim_start().len());
    let token = &line[token_start..];
    let number_end = numeric_prefix_len(token)?;
    let unit_start = token[number_end..]
        .rfind(['/', '*'])
        .map_or(number_end, |position| number_end + position + 1);
    let typed = &token[unit_start..];
    if !typed.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return None;
    }
    let replace = ByteRange {
        start: line_start + token_start + unit_start,
        end: offset,
    };
    let mut units = HashSet::new();
    for quantity in &index.quantities {
        units.insert(quantity.unit.clone());
        if let Some(unit) = &quantity.super_unit {
            units.insert(unit.clone());
        }
    }
    let mut items = units
        .into_iter()
        .filter(|unit| unit.starts_with(typed))
        .map(|unit| CompletionItem {
            label: unit.clone(),
            kind: CompletionKind::Unit,
            detail: "unit used in this document".into(),
            insert_text: unit,
            replace,
            sort_text: "0".into(),
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| left.label.cmp(&right.label));
    Some(items)
}

fn complete_values_and_paths(
    line: &str,
    line_start: usize,
    offset: usize,
    index: &SemanticIndex,
) -> Vec<CompletionItem> {
    let Some(equals) = line.rfind('=') else {
        return Vec::new();
    };
    let value_start =
        equals + 1 + (line[equals + 1..].len() - line[equals + 1..].trim_start().len());
    let typed = &line[value_start..];
    if !typed.is_empty() && !is_path_prefix(typed) {
        return Vec::new();
    }
    let replace = ByteRange {
        start: line_start + value_start,
        end: offset,
    };
    let current_table = index
        .definitions
        .iter()
        .rev()
        .find(|definition| definition.kind == DefinitionKind::Table)
        .map(|definition| definition.path.as_slice())
        .unwrap_or(&[]);
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for definition in &index.definitions {
        if definition.kind == DefinitionKind::Enum {
            let label = definition.path.join(".");
            if label.starts_with(typed) && seen.insert((CompletionKind::Enum, label.clone())) {
                items.push(CompletionItem {
                    label: label.clone(),
                    kind: CompletionKind::Enum,
                    detail: "ATML enum".into(),
                    insert_text: format!("{label}::"),
                    replace,
                    sort_text: locality_sort(&definition.path, current_table, 0),
                });
            }
        }
        if matches!(definition.kind, DefinitionKind::Key | DefinitionKind::Table)
            && definition.path.len() >= 2
        {
            let label = definition.path.join(".");
            if label.starts_with(typed) && seen.insert((CompletionKind::Key, label.clone())) {
                items.push(CompletionItem {
                    label: label.clone(),
                    kind: CompletionKind::Key,
                    detail: format!("ATML reference ({:?})", definition.value_type),
                    insert_text: label,
                    replace,
                    sort_text: locality_sort(&definition.path, current_table, 1),
                });
            }
        }
    }

    if typed.is_empty() {
        for (order, (label, detail)) in [
            ("\"\"", "string"),
            ("0", "integer"),
            ("0.0", "float"),
            ("true", "boolean"),
            ("false", "boolean"),
            ("[]", "array"),
            ("{}", "inline table"),
            ("0unit", "ATML quantity"),
        ]
        .into_iter()
        .enumerate()
        {
            items.push(CompletionItem {
                label: label.into(),
                kind: CompletionKind::Value,
                detail: detail.into(),
                insert_text: label.into(),
                replace,
                sort_text: format!("2{order:02}"),
            });
        }
    }

    items.sort_by(|left, right| {
        left.sort_text
            .cmp(&right.sort_text)
            .then_with(|| left.label.cmp(&right.label))
    });
    items
}

fn locality_sort(path: &[String], current_table: &[String], group: u8) -> String {
    let local = !current_table.is_empty() && path.starts_with(current_table);
    format!("{group}{}", if local { 0 } else { 1 })
}

fn numeric_prefix_len(token: &str) -> Option<usize> {
    let bytes = token.as_bytes();
    let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let integer_start = end;
    while matches!(bytes.get(end), Some(b'0'..=b'9' | b'_')) {
        end += 1;
    }
    if end == integer_start {
        return None;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while matches!(bytes.get(end), Some(b'0'..=b'9' | b'_')) {
            end += 1;
        }
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let exponent = end;
        let mut candidate = end + 1;
        if matches!(bytes.get(candidate), Some(b'+' | b'-')) {
            candidate += 1;
        }
        let digit_start = candidate;
        while matches!(bytes.get(candidate), Some(b'0'..=b'9' | b'_')) {
            candidate += 1;
        }
        if candidate > digit_start {
            end = candidate;
        } else {
            end = exponent;
        }
    }
    Some(end)
}

fn is_path_prefix(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

fn in_comment_or_string(line: &str) -> bool {
    let mut basic = false;
    let mut literal = false;
    let mut escaped = false;
    for ch in line.chars() {
        if basic {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                basic = false;
            }
        } else if literal {
            if ch == '\'' {
                literal = false;
            }
        } else {
            match ch {
                '#' => return true,
                '"' => basic = true,
                '\'' => literal = true,
                _ => {}
            }
        }
    }
    basic || literal
}

fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
