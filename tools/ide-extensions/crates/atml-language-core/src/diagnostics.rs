use std::collections::HashSet;

use toml_dom::Document;

use crate::{
    semantic::parse_path_reference, ByteRange, CycleKind, DefinitionKind, Diagnostic,
    DiagnosticSeverity, ReferenceKind, SemanticIndex,
};

pub(crate) fn semantic_diagnostics(index: &SemanticIndex) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for reference in &index.references {
        match reference.kind {
            ReferenceKind::Path if reference.target.is_none() => diagnostics.push(Diagnostic {
                code: "atml.reference.unknown-target",
                severity: DiagnosticSeverity::Error,
                message: format!(
                    "unknown ATML reference target '{}'",
                    reference.target_path.join(".")
                ),
                range: reference.range,
            }),
            ReferenceKind::EnumMember if reference.target.is_none() => {
                let enum_path = &reference.target_path[..reference.target_path.len() - 1];
                let member = reference.target_path.last().cloned().unwrap_or_default();
                let enum_exists = index.definitions.iter().any(|definition| {
                    definition.kind == DefinitionKind::Enum && definition.path == enum_path
                });
                diagnostics.push(Diagnostic {
                    code: if enum_exists {
                        "atml.enum.unknown-member"
                    } else {
                        "atml.enum.unknown-definition"
                    },
                    severity: DiagnosticSeverity::Error,
                    message: if enum_exists {
                        format!("enum '{}' has no member '{}'", enum_path.join("."), member)
                    } else {
                        format!("unknown enum '{}'", enum_path.join("."))
                    },
                    range: reference.range,
                });
            }
            ReferenceKind::EnumMember => {
                if let Some(definition) = reference.target.and_then(|id| index.definition(id)) {
                    let enum_path = &reference.target_path[..reference.target_path.len() - 1];
                    let enum_definition = index.definitions.iter().find(|candidate| {
                        candidate.kind == DefinitionKind::Enum && candidate.path == enum_path
                    });
                    if enum_definition.is_some_and(|enum_definition| {
                        enum_definition.range.start > reference.range.start
                    }) {
                        diagnostics.push(Diagnostic {
                            code: "atml.enum.used-before-definition",
                            severity: DiagnosticSeverity::Error,
                            message: format!(
                                "enum '{}' is used before its definition",
                                enum_path.join(".")
                            ),
                            range: reference.range,
                        });
                    }
                    let _ = definition;
                }
            }
            _ => {}
        }
    }

    for edge in &index.inheritance {
        match edge.parent.and_then(|id| index.definition(id)) {
            None => diagnostics.push(Diagnostic {
                code: "atml.inheritance.unknown-parent",
                severity: DiagnosticSeverity::Error,
                message: format!("unknown ATML parent table '{}'", edge.parent_path.join(".")),
                range: edge.range,
            }),
            Some(parent) if !matches!(parent.kind, DefinitionKind::Table) => {
                diagnostics.push(Diagnostic {
                    code: "atml.inheritance.invalid-parent-type",
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "ATML parent '{}' must be a standard table",
                        edge.parent_path.join(".")
                    ),
                    range: edge.range,
                });
            }
            _ => {}
        }
    }

    let mut reported_cycles = HashSet::new();
    for cycle in &index.cycles {
        let code = match cycle.kind {
            CycleKind::PathReference => "atml.reference.cycle",
            CycleKind::Inheritance => "atml.inheritance.cycle",
        };
        let message = format!(
            "cyclic {}: {}",
            match cycle.kind {
                CycleKind::PathReference => "ATML path reference",
                CycleKind::Inheritance => "ATML table inheritance",
            },
            cycle
                .paths
                .iter()
                .map(|path| path.join("."))
                .collect::<Vec<_>>()
                .join(" -> ")
        );
        for path in &cycle.paths[..cycle.paths.len().saturating_sub(1)] {
            let range = match cycle.kind {
                CycleKind::PathReference => index
                    .references
                    .iter()
                    .find(|reference| {
                        reference.kind == ReferenceKind::Path && reference.source_path == *path
                    })
                    .map(|reference| reference.range),
                CycleKind::Inheritance => index
                    .inheritance
                    .iter()
                    .find(|edge| edge.child_path == *path)
                    .map(|edge| edge.range),
            };
            if let Some(range) = range {
                if reported_cycles.insert((code, range.start, range.end)) {
                    diagnostics.push(Diagnostic {
                        code,
                        severity: DiagnosticSeverity::Error,
                        message: message.clone(),
                        range,
                    });
                }
            }
        }
    }

    diagnostics.sort_by_key(|diagnostic| (diagnostic.range.start, diagnostic.code));
    diagnostics
}

pub(crate) fn recover_reference_document(source: &str) -> Option<Document> {
    let ranges = reference_value_ranges(source);
    if ranges.is_empty() {
        return None;
    }
    let mut recovered = source.to_owned();
    for range in ranges.into_iter().rev() {
        let width = range.end - range.start;
        let replacement = format!("\"{}\"", "x".repeat(width.saturating_sub(2)));
        recovered.replace_range(range.start..range.end, &replacement);
    }
    Document::parse_atml(&recovered).ok()
}

fn reference_value_ranges(source: &str) -> Vec<ByteRange> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum StringState {
        None,
        Basic,
        Literal,
        MultiBasic,
        MultiLiteral,
    }

    let bytes = source.as_bytes();
    let mut state = StringState::None;
    let mut escaped = false;
    let mut comment = false;
    let mut expect_value = false;
    let mut array_depth = 0_u32;
    let mut header_depth = 0_u32;
    let mut ranges = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if comment {
            if byte == b'\n' {
                comment = false;
            }
            index += 1;
            continue;
        }
        match state {
            StringState::Basic => {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    state = StringState::None;
                }
            }
            StringState::Literal => {
                if byte == b'\'' {
                    state = StringState::None;
                }
            }
            StringState::MultiBasic => {
                if bytes.get(index..index + 3) == Some(b"\"\"\"") {
                    state = StringState::None;
                    index += 2;
                }
            }
            StringState::MultiLiteral => {
                if bytes.get(index..index + 3) == Some(b"'''") {
                    state = StringState::None;
                    index += 2;
                }
            }
            StringState::None => match byte {
                b'#' => comment = true,
                b'"' if bytes.get(index..index + 3) == Some(b"\"\"\"") => {
                    state = StringState::MultiBasic;
                    expect_value = false;
                    index += 2;
                }
                b'\'' if bytes.get(index..index + 3) == Some(b"'''") => {
                    state = StringState::MultiLiteral;
                    expect_value = false;
                    index += 2;
                }
                b'"' => {
                    state = StringState::Basic;
                    expect_value = false;
                }
                b'\'' => {
                    state = StringState::Literal;
                    expect_value = false;
                }
                b'=' => expect_value = true,
                b'[' if expect_value || array_depth > 0 => {
                    array_depth += 1;
                    expect_value = true;
                }
                b'[' => header_depth += 1,
                b']' if array_depth > 0 => {
                    array_depth -= 1;
                    expect_value = false;
                }
                b']' if header_depth > 0 => header_depth -= 1,
                b',' if array_depth > 0 && header_depth == 0 => expect_value = true,
                b'{' if expect_value => expect_value = false,
                byte if expect_value && (byte.is_ascii_alphabetic() || byte == b'_') => {
                    let start = index;
                    let mut end = start + 1;
                    while bytes.get(end).is_some_and(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
                    }) {
                        end += 1;
                    }
                    if parse_path_reference(&source[start..end]).is_some() {
                        ranges.push(ByteRange { start, end });
                    }
                    expect_value = false;
                    index = end - 1;
                }
                byte if expect_value && !byte.is_ascii_whitespace() => expect_value = false,
                _ => {}
            },
        }
        index += 1;
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_preserves_offsets_and_ignores_strings_and_comments() {
        let source = "text = \"a.b\"\n# ignored = a.b\nactual = missing.value\narray = [\n  one.value,\n  two.value,\n]\n";
        let recovered = recover_reference_document(source).unwrap();
        let serialized = recovered.serialize();
        assert_eq!(serialized.len(), source.len());
    }
}
