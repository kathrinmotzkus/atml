use toml_dom::{QuantityMagnitude, QuantitySeparator};

use crate::{
    ByteRange, Definition, DefinitionKind, QuantityOccurrence, Reference, ReferenceKind,
    SemanticIndex, ValueType,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverResult {
    pub range: ByteRange,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationTarget {
    pub range: ByteRange,
    pub selection_range: ByteRange,
}

/// Return compact, Markdown-safe semantic information at a UTF-8 byte offset.
pub fn hover(source: &str, index: &SemanticIndex, offset: usize) -> Option<HoverResult> {
    if let Some(quantity) = index
        .quantities
        .iter()
        .find(|quantity| contains(quantity.range, offset))
    {
        return Some(quantity_hover(quantity));
    }
    if let Some(reference) = index
        .references
        .iter()
        .find(|reference| contains(reference.range, offset))
    {
        return reference_hover(source, index, reference);
    }
    if let Some(edge) = index
        .inheritance
        .iter()
        .find(|edge| contains(edge.range, offset))
    {
        let parent = edge.parent.and_then(|id| index.definition(id));
        let mut markdown = format!(
            "**Inherited table** {}\n\nChild: {}",
            code(&edge.parent_path.join(".")),
            code(&edge.child_path.join("."))
        );
        if parent.is_some() {
            append_inherited_values(source, index, &edge.child_path, &mut markdown);
        }
        return Some(HoverResult {
            range: edge.range,
            markdown,
        });
    }
    let definition = index
        .definitions
        .iter()
        .find(|definition| contains(definition.selection_range, offset))?;
    Some(definition_hover(source, index, definition))
}

/// Resolve the authored target under the cursor. For a transitive path
/// reference this intentionally returns the next link, so repeated navigation
/// keeps the chain visible.
pub fn goto_definition(index: &SemanticIndex, offset: usize) -> Option<NavigationTarget> {
    let definition = symbol_definition(index, offset)?;
    Some(NavigationTarget {
        range: definition.range,
        selection_range: definition.selection_range,
    })
}

/// Find references to the definition under the cursor in the current document.
pub fn find_references(
    index: &SemanticIndex,
    offset: usize,
    include_declaration: bool,
) -> Vec<ByteRange> {
    let Some(definition) = symbol_definition(index, offset) else {
        return Vec::new();
    };
    let mut ranges = Vec::new();
    if include_declaration {
        ranges.push(definition.selection_range);
    }
    for reference in &index.references {
        let matches = match definition.kind {
            DefinitionKind::Enum => {
                reference.kind == ReferenceKind::EnumMember
                    && reference.target_path.starts_with(&definition.path)
            }
            DefinitionKind::Table | DefinitionKind::ArrayOfTables => {
                reference.kind == ReferenceKind::Path
                    && reference.target_path.starts_with(&definition.path)
            }
            _ => {
                reference.target == Some(definition.id)
                    || reference.resolved_target == Some(definition.id)
            }
        };
        if matches {
            ranges.push(reference.range);
        }
    }
    if definition.kind == DefinitionKind::Table {
        ranges.extend(
            index
                .inheritance
                .iter()
                .filter(|edge| edge.parent == Some(definition.id))
                .map(|edge| edge.range),
        );
    }
    ranges.sort_by_key(|range| (range.start, range.end));
    ranges.dedup();
    ranges
}

fn symbol_definition(index: &SemanticIndex, offset: usize) -> Option<&Definition> {
    if let Some(definition) = index
        .definitions
        .iter()
        .find(|definition| contains(definition.selection_range, offset))
    {
        return Some(definition);
    }
    if let Some(reference) = index
        .references
        .iter()
        .find(|reference| contains(reference.range, offset))
    {
        return reference.target.and_then(|id| index.definition(id));
    }
    index
        .inheritance
        .iter()
        .find(|edge| contains(edge.range, offset))
        .and_then(|edge| edge.parent)
        .and_then(|id| index.definition(id))
}

fn definition_hover(source: &str, index: &SemanticIndex, definition: &Definition) -> HoverResult {
    let title = match definition.kind {
        DefinitionKind::Key => "Key",
        DefinitionKind::Table => "Table",
        DefinitionKind::ArrayOfTables => "Array of tables",
        DefinitionKind::Enum => "Enum",
        DefinitionKind::EnumMember => "Enum member",
    };
    let mut markdown = format!(
        "**{title}** {}\n\nType: `{}`\n\nDefinition: {}",
        code(&definition.path.join(".")),
        value_type_name(definition.value_type),
        source_location(source, definition.selection_range.start)
    );
    if definition.kind == DefinitionKind::Enum {
        append_enum_members(index, &definition.path, &mut markdown);
    }
    if definition.kind == DefinitionKind::Table {
        append_inherited_values(source, index, &definition.path, &mut markdown);
    }
    HoverResult {
        range: definition.selection_range,
        markdown,
    }
}

fn quantity_hover(quantity: &QuantityOccurrence) -> HoverResult {
    let (unit, exponent) = split_exponent(&quantity.unit);
    let magnitude = match quantity.magnitude {
        QuantityMagnitude::Integer(value) => value.to_string(),
        QuantityMagnitude::Float(value) => value.to_string(),
    };
    let mut markdown = format!(
        "**ATML quantity**\n\n- Magnitude: `{}`\n- Unit: {}\n- Exponent: {}",
        safe_code_text(&magnitude),
        code(unit),
        exponent.map_or_else(|| "none".into(), code)
    );
    if let (Some(separator), Some(super_unit)) = (quantity.separator, &quantity.super_unit) {
        let (super_name, super_exponent) = split_exponent(super_unit);
        let relation = match separator {
            QuantitySeparator::Per => "per",
            QuantitySeparator::Times => "times",
        };
        markdown.push_str(&format!(
            "\n- Super-unit: {} ({relation}, exponent {})",
            code(super_name),
            super_exponent.map_or_else(|| "none".into(), code)
        ));
    } else {
        markdown.push_str("\n- Super-unit: none");
    }
    HoverResult {
        range: quantity.range,
        markdown,
    }
}

fn reference_hover(
    source: &str,
    index: &SemanticIndex,
    reference: &Reference,
) -> Option<HoverResult> {
    match reference.kind {
        ReferenceKind::EnumMember => {
            let enum_path = &reference.target_path[..reference.target_path.len().saturating_sub(1)];
            let definition = index.definitions.iter().find(|definition| {
                definition.kind == DefinitionKind::Enum && definition.path == enum_path
            });
            let enum_value = format!(
                "{}::{}",
                enum_path.join("."),
                reference.target_path.last().cloned().unwrap_or_default()
            );
            let mut markdown = format!(
                "**Enum value** {}\n\nDefined by: {}{}",
                code(&enum_value),
                code(&enum_path.join(".")),
                definition.map_or_else(String::new, |definition| format!(
                    " ({})",
                    source_location(source, definition.selection_range.start)
                ))
            );
            append_enum_members(index, enum_path, &mut markdown);
            Some(HoverResult {
                range: reference.range,
                markdown,
            })
        }
        ReferenceKind::Path => {
            let direct = reference.target.and_then(|id| index.definition(id));
            let resolved = reference
                .resolved_target
                .and_then(|id| index.definition(id));
            let mut markdown = format!(
                "**Path reference** {}\n\nDirect target: {}",
                code(&reference.target_path.join(".")),
                direct.map_or_else(|| "unresolved".into(), |item| code(&item.path.join(".")))
            );
            if let Some(resolved) = resolved {
                markdown.push_str(&format!(
                    "\n\nResolved target: {}",
                    code(&resolved.path.join("."))
                ));
                if let Some(value) = definition_value(source, resolved) {
                    markdown.push_str(&format!("\n\nResolved value: {}", code(value)));
                }
            }
            Some(HoverResult {
                range: reference.range,
                markdown,
            })
        }
        ReferenceKind::Inheritance => None,
    }
}

fn append_enum_members(index: &SemanticIndex, enum_path: &[String], markdown: &mut String) {
    let members = index
        .definitions
        .iter()
        .filter(|definition| {
            definition.kind == DefinitionKind::EnumMember
                && definition.path.len() == enum_path.len() + 1
                && definition.path.starts_with(enum_path)
        })
        .map(|definition| code(&definition.name))
        .collect::<Vec<_>>();
    if !members.is_empty() {
        markdown.push_str(&format!("\n\nAllowed: {}", members.join(", ")));
    }
}

fn append_inherited_values(
    source: &str,
    index: &SemanticIndex,
    child_path: &[String],
    markdown: &mut String,
) {
    let explicit = index
        .definitions
        .iter()
        .filter(|definition| {
            definition.kind == DefinitionKind::Key && definition.path.starts_with(child_path)
        })
        .map(|definition| definition.path[child_path.len()..].to_vec())
        .collect::<std::collections::HashSet<_>>();
    let mut seen = explicit;
    let mut inherited = Vec::new();
    for parent in index.inheritance_chain(child_path) {
        for definition in index.definitions.iter().filter(|definition| {
            definition.kind == DefinitionKind::Key
                && definition.path.len() > parent.path.len()
                && definition.path.starts_with(&parent.path)
        }) {
            let suffix = definition.path[parent.path.len()..].to_vec();
            if seen.insert(suffix.clone()) {
                let value = definition_value(source, definition).unwrap_or("unknown");
                inherited.push(format!(
                    "{} = {} from {}",
                    code(&suffix.join(".")),
                    code(value),
                    code(&parent.path.join("."))
                ));
            }
        }
    }
    if !inherited.is_empty() {
        markdown.push_str("\n\nInherited values:\n\n- ");
        markdown.push_str(&inherited.join("\n- "));
    }
}

fn definition_value<'a>(source: &'a str, definition: &Definition) -> Option<&'a str> {
    source
        .get(definition.range.start..definition.range.end)?
        .split_once('=')
        .map(|(_, value)| value.trim())
}

fn split_exponent(unit: &str) -> (&str, Option<&str>) {
    if let Some(index) = unit.find('^') {
        return (&unit[..index], Some(&unit[index..]));
    }
    let index = unit
        .char_indices()
        .find(|(_, ch)| matches!(ch, '¹' | '²' | '³' | '⁰' | '⁴'..='⁹' | '⁻'))
        .map(|(index, _)| index);
    index.map_or((unit, None), |index| (&unit[..index], Some(&unit[index..])))
}

fn value_type_name(value_type: ValueType) -> &'static str {
    match value_type {
        ValueType::String => "string",
        ValueType::Integer => "integer",
        ValueType::Float => "float",
        ValueType::Boolean => "boolean",
        ValueType::OffsetDateTime => "offset date-time",
        ValueType::LocalDateTime => "local date-time",
        ValueType::LocalDate => "local date",
        ValueType::LocalTime => "local time",
        ValueType::Array => "array",
        ValueType::Table => "table",
        ValueType::Quantity => "quantity",
        ValueType::EnumReference => "enum reference",
        ValueType::EnumDefinition => "enum definition",
        ValueType::Unknown => "unknown",
    }
}

fn contains(range: ByteRange, offset: usize) -> bool {
    range.start <= offset && offset < range.end
}

fn code(value: &str) -> String {
    format!("`{}`", safe_code_text(value))
}

fn safe_code_text(value: &str) -> String {
    value.replace('`', "'").replace(['\r', '\n'], " ")
}

fn source_location(source: &str, offset: usize) -> String {
    let prefix = &source[..offset.min(source.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let column = prefix[line_start..].chars().count() + 1;
    format!("line {line}, column {column}")
}
