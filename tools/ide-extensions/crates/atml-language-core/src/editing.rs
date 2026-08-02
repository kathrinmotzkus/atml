use crate::{ByteRange, Definition, DefinitionKind, Diagnostic, ReferenceKind, SemanticIndex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticTokenKind {
    Property,
    Table,
    Enum,
    EnumMember,
    Reference,
    Number,
    Unit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticToken {
    pub range: ByteRange,
    pub kind: SemanticTokenKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameTarget {
    pub range: ByteRange,
    pub placeholder: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChange {
    pub range: ByteRange,
    pub new_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameError {
    NotRenameable,
    InvalidName,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickFix {
    pub title: String,
    pub diagnostic_code: &'static str,
    pub change: TextChange,
}

pub fn semantic_tokens(source: &str, index: &SemanticIndex) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();
    for definition in &index.definitions {
        let kind = match definition.kind {
            DefinitionKind::Key => SemanticTokenKind::Property,
            DefinitionKind::Table | DefinitionKind::ArrayOfTables => SemanticTokenKind::Table,
            DefinitionKind::Enum => SemanticTokenKind::Enum,
            DefinitionKind::EnumMember => SemanticTokenKind::EnumMember,
        };
        let range = last_path_segment(source, definition.selection_range, '.');
        tokens.push(SemanticToken { range, kind });
    }
    for reference in &index.references {
        match reference.kind {
            ReferenceKind::EnumMember => {
                if let Some(separator) =
                    source[reference.range.start..reference.range.end].rfind("::")
                {
                    tokens.push(SemanticToken {
                        range: ByteRange {
                            start: reference.range.start,
                            end: reference.range.start + separator,
                        },
                        kind: SemanticTokenKind::Enum,
                    });
                    tokens.push(SemanticToken {
                        range: ByteRange {
                            start: reference.range.start + separator + 2,
                            end: reference.range.end,
                        },
                        kind: SemanticTokenKind::EnumMember,
                    });
                }
            }
            ReferenceKind::Path => tokens.push(SemanticToken {
                range: reference.range,
                kind: SemanticTokenKind::Reference,
            }),
            ReferenceKind::Inheritance => {}
        }
    }
    tokens.extend(index.inheritance.iter().map(|edge| SemanticToken {
        range: edge.range,
        kind: SemanticTokenKind::Table,
    }));
    for quantity in &index.quantities {
        let authored = &source[quantity.range.start..quantity.range.end];
        let suffix_len = quantity.unit.len()
            + quantity.separator.map_or(0, |_| 1)
            + quantity.super_unit.as_ref().map_or(0, String::len);
        let number_end = authored.len().saturating_sub(suffix_len);
        tokens.push(SemanticToken {
            range: ByteRange {
                start: quantity.range.start,
                end: quantity.range.start + number_end,
            },
            kind: SemanticTokenKind::Number,
        });
        let unit_start = quantity.range.start + number_end;
        if let Some(separator) = authored[number_end..].find(['/', '*']) {
            let separator = unit_start + separator;
            tokens.push(SemanticToken {
                range: ByteRange {
                    start: unit_start,
                    end: separator,
                },
                kind: SemanticTokenKind::Unit,
            });
            tokens.push(SemanticToken {
                range: ByteRange {
                    start: separator + 1,
                    end: quantity.range.end,
                },
                kind: SemanticTokenKind::Unit,
            });
        } else {
            tokens.push(SemanticToken {
                range: ByteRange {
                    start: unit_start,
                    end: quantity.range.end,
                },
                kind: SemanticTokenKind::Unit,
            });
        }
    }
    tokens.retain(|token| token.range.start < token.range.end);
    tokens.sort_by_key(|token| (token.range.start, token.range.end));
    tokens.dedup_by_key(|token| (token.range.start, token.range.end));
    tokens
}

pub fn prepare_rename(source: &str, index: &SemanticIndex, offset: usize) -> Option<RenameTarget> {
    let definition = rename_definition(source, index, offset)?;
    if !matches!(
        definition.kind,
        DefinitionKind::Key | DefinitionKind::Table | DefinitionKind::Enum
    ) {
        return None;
    }
    let range = rename_occurrence_range(source, index, offset, definition)?;
    if !contains(range, offset) {
        return None;
    }
    let placeholder = source.get(range.start..range.end)?;
    is_bare_name(placeholder).then(|| RenameTarget {
        range,
        placeholder: placeholder.to_owned(),
    })
}

pub fn rename(
    source: &str,
    index: &SemanticIndex,
    offset: usize,
    new_name: &str,
) -> Result<Vec<TextChange>, RenameError> {
    if !is_bare_name(new_name) {
        return Err(RenameError::InvalidName);
    }
    let definition = rename_definition(source, index, offset).ok_or(RenameError::NotRenameable)?;
    if !matches!(
        definition.kind,
        DefinitionKind::Key | DefinitionKind::Table | DefinitionKind::Enum
    ) {
        return Err(RenameError::NotRenameable);
    }
    let selected = rename_occurrence_range(source, index, offset, definition)
        .filter(|range| contains(*range, offset))
        .ok_or(RenameError::NotRenameable)?;
    if !source
        .get(selected.start..selected.end)
        .is_some_and(is_bare_name)
    {
        return Err(RenameError::NotRenameable);
    }
    let declaration = declaration_name_range(source, definition);
    if !source
        .get(declaration.start..declaration.end)
        .is_some_and(is_bare_name)
    {
        return Err(RenameError::NotRenameable);
    }
    let mut new_path = definition.path.clone();
    *new_path.last_mut().ok_or(RenameError::NotRenameable)? = new_name.to_owned();
    if index
        .definitions
        .iter()
        .any(|candidate| candidate.id != definition.id && candidate.path == new_path)
    {
        return Err(RenameError::Conflict);
    }

    let mut ranges = vec![declaration];
    match definition.kind {
        DefinitionKind::Key => {
            ranges.extend(
                index
                    .references
                    .iter()
                    .filter(|reference| reference.target == Some(definition.id))
                    .map(|reference| last_path_segment(source, reference.range, '.')),
            );
        }
        DefinitionKind::Enum => {
            for reference in index.references.iter().filter(|reference| {
                reference.kind == ReferenceKind::EnumMember
                    && reference.target_path.starts_with(&definition.path)
            }) {
                let enum_end = source[reference.range.start..reference.range.end]
                    .rfind("::")
                    .map(|position| reference.range.start + position)
                    .ok_or(RenameError::NotRenameable)?;
                ranges.push(last_path_segment(
                    source,
                    ByteRange {
                        start: reference.range.start,
                        end: enum_end,
                    },
                    '.',
                ));
            }
        }
        DefinitionKind::Table => {
            let segment = definition.path.len().saturating_sub(1);
            for reference in index
                .references
                .iter()
                .filter(|reference| reference.target_path.starts_with(&definition.path))
            {
                let range = if reference.kind == ReferenceKind::EnumMember {
                    let separator = source[reference.range.start..reference.range.end]
                        .rfind("::")
                        .ok_or(RenameError::NotRenameable)?;
                    ByteRange {
                        start: reference.range.start,
                        end: reference.range.start + separator,
                    }
                } else {
                    reference.range
                };
                ranges.push(path_segment_range(source, range, segment)?);
            }
            for edge in index
                .inheritance
                .iter()
                .filter(|edge| edge.parent_path.starts_with(&definition.path))
            {
                ranges.push(path_segment_range(source, edge.range, segment)?);
            }
            for child in index.definitions.iter().filter(|candidate| {
                matches!(
                    candidate.kind,
                    DefinitionKind::Table | DefinitionKind::ArrayOfTables
                ) && candidate.path.starts_with(&definition.path)
                    && candidate.path.len() > definition.path.len()
            }) {
                ranges.push(path_segment_range(source, child.selection_range, segment)?);
            }
        }
        _ => return Err(RenameError::NotRenameable),
    }
    ranges.sort_by_key(|range| (range.start, range.end));
    ranges.dedup();
    Ok(ranges
        .into_iter()
        .map(|range| TextChange {
            range,
            new_text: new_name.to_owned(),
        })
        .collect())
}

pub fn quick_fixes(
    source: &str,
    index: &SemanticIndex,
    diagnostics: &[Diagnostic],
    requested_range: ByteRange,
) -> Vec<QuickFix> {
    diagnostics
        .iter()
        .filter(|diagnostic| overlaps(diagnostic.range, requested_range))
        .filter_map(|diagnostic| quick_fix(source, index, diagnostic))
        .collect()
}

fn quick_fix(source: &str, index: &SemanticIndex, diagnostic: &Diagnostic) -> Option<QuickFix> {
    let authored = source.get(diagnostic.range.start..diagnostic.range.end)?;
    let (range, replacement) = match diagnostic.code {
        "atml.enum.unknown-member" => {
            let separator = authored.rfind("::")?;
            let enum_path = authored[..separator].split('.').collect::<Vec<_>>();
            let typed = &authored[separator + 2..];
            let candidates = index.definitions.iter().filter(|definition| {
                definition.kind == DefinitionKind::EnumMember
                    && definition.path.len() == enum_path.len() + 1
                    && definition.path[..enum_path.len()]
                        .iter()
                        .map(String::as_str)
                        .eq(enum_path.iter().copied())
                    && definition.name.eq_ignore_ascii_case(typed)
            });
            let candidate = exactly_one(candidates)?.name.clone();
            (
                ByteRange {
                    start: diagnostic.range.start + separator + 2,
                    end: diagnostic.range.end,
                },
                candidate,
            )
        }
        "atml.enum.unknown-definition" => {
            let separator = authored.rfind("::")?;
            let typed = &authored[..separator];
            let candidate = exactly_one(index.definitions.iter().filter(|definition| {
                definition.kind == DefinitionKind::Enum
                    && definition.path.join(".").eq_ignore_ascii_case(typed)
            }))?;
            (
                ByteRange {
                    start: diagnostic.range.start,
                    end: diagnostic.range.start + separator,
                },
                candidate.path.join("."),
            )
        }
        "atml.reference.unknown-target" => {
            let candidate = exactly_one(index.definitions.iter().filter(|definition| {
                matches!(definition.kind, DefinitionKind::Key | DefinitionKind::Table)
                    && definition.path.join(".").eq_ignore_ascii_case(authored)
            }))?;
            (diagnostic.range, candidate.path.join("."))
        }
        "atml.inheritance.unknown-parent" => {
            let candidate = exactly_one(index.definitions.iter().filter(|definition| {
                definition.kind == DefinitionKind::Table
                    && definition.path.join(".").eq_ignore_ascii_case(authored)
            }))?;
            (diagnostic.range, candidate.path.join("."))
        }
        _ => return None,
    };
    Some(QuickFix {
        title: format!("Change to '{replacement}'"),
        diagnostic_code: diagnostic.code,
        change: TextChange {
            range,
            new_text: replacement,
        },
    })
}

fn exactly_one<T>(mut values: impl Iterator<Item = T>) -> Option<T> {
    let value = values.next()?;
    values.next().is_none().then_some(value)
}

fn declaration_name_range(source: &str, definition: &Definition) -> ByteRange {
    last_path_segment(source, definition.selection_range, '.')
}

fn rename_occurrence_range(
    source: &str,
    index: &SemanticIndex,
    offset: usize,
    definition: &Definition,
) -> Option<ByteRange> {
    if contains(definition.selection_range, offset) {
        return Some(declaration_name_range(source, definition));
    }
    if let Some(reference) = index
        .references
        .iter()
        .find(|reference| contains(reference.range, offset))
    {
        return match definition.kind {
            DefinitionKind::Key => Some(last_path_segment(source, reference.range, '.')),
            DefinitionKind::Enum => {
                let separator = source[reference.range.start..reference.range.end].rfind("::")?;
                Some(last_path_segment(
                    source,
                    ByteRange {
                        start: reference.range.start,
                        end: reference.range.start + separator,
                    },
                    '.',
                ))
            }
            DefinitionKind::Table => path_segment_range(
                source,
                reference.range,
                definition.path.len().saturating_sub(1),
            )
            .ok(),
            _ => None,
        };
    }
    let edge = index
        .inheritance
        .iter()
        .find(|edge| contains(edge.range, offset))?;
    (definition.kind == DefinitionKind::Table).then(|| last_path_segment(source, edge.range, '.'))
}

fn rename_definition<'a>(
    source: &str,
    index: &'a SemanticIndex,
    offset: usize,
) -> Option<&'a Definition> {
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
        if reference.kind == ReferenceKind::EnumMember {
            let separator = source[reference.range.start..reference.range.end].rfind("::")?;
            let enum_range = ByteRange {
                start: reference.range.start,
                end: reference.range.start + separator,
            };
            let name_range = last_path_segment(source, enum_range, '.');
            if !contains(name_range, offset) {
                return None;
            }
            let enum_path = &reference.target_path[..reference.target_path.len().saturating_sub(1)];
            return index.definitions.iter().find(|definition| {
                definition.kind == DefinitionKind::Enum && definition.path == enum_path
            });
        }
        let segment = path_segment_at_offset(source, reference.range, offset)?;
        let target_path = &reference.target_path[..=segment];
        return index.definitions.iter().find(|definition| {
            definition.path == target_path
                && if segment + 1 == reference.target_path.len() {
                    definition.kind == DefinitionKind::Key
                } else {
                    definition.kind == DefinitionKind::Table
                }
        });
    }
    index
        .inheritance
        .iter()
        .find(|edge| contains(edge.range, offset))
        .and_then(|edge| edge.parent)
        .and_then(|id| index.definition(id))
}

fn path_segment_at_offset(source: &str, range: ByteRange, offset: usize) -> Option<usize> {
    let text = &source[range.start..range.end];
    let mut start = range.start;
    for (segment, part) in text.split('.').enumerate() {
        let part_range = ByteRange {
            start,
            end: start + part.len(),
        };
        if contains(part_range, offset) {
            return Some(segment);
        }
        start = part_range.end + 1;
    }
    None
}

fn last_path_segment(source: &str, range: ByteRange, separator: char) -> ByteRange {
    let text = &source[range.start..range.end];
    let start = text.rfind(separator).map_or(range.start, |index| {
        range.start + index + separator.len_utf8()
    });
    ByteRange {
        start,
        end: range.end,
    }
}

fn path_segment_range(
    source: &str,
    range: ByteRange,
    segment: usize,
) -> Result<ByteRange, RenameError> {
    let text = &source[range.start..range.end];
    let mut start = 0;
    for (index, part) in text.split('.').enumerate() {
        let end = start + part.len();
        if index == segment {
            return Ok(ByteRange {
                start: range.start + start,
                end: range.start + end,
            });
        }
        start = end + 1;
    }
    Err(RenameError::NotRenameable)
}

fn is_bare_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

fn overlaps(left: ByteRange, right: ByteRange) -> bool {
    left.start < right.end && right.start < left.end
}

fn contains(range: ByteRange, offset: usize) -> bool {
    range.start <= offset && offset < range.end
}
