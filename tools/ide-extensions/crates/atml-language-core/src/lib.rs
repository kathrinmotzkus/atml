//! Editor-independent analysis for ATML documents.

mod completion;
mod diagnostics;
mod editing;
mod navigation;
mod semantic;

pub use completion::{complete, CompletionItem, CompletionKind};
pub use editing::{
    prepare_rename, quick_fixes, rename, semantic_tokens, QuickFix, RenameError, RenameTarget,
    SemanticToken, SemanticTokenKind, TextChange,
};
pub use navigation::{find_references, goto_definition, hover, HoverResult, NavigationTarget};
pub use semantic::{
    CycleKind, Definition, DefinitionId, DefinitionKind, InheritanceEdge, QuantityOccurrence,
    Reference, ReferenceKind, SemanticCycle, SemanticIndex, ValueType,
};

use toml_dom::{Document, DocumentItem, TomlError, TomlErrorKind};

/// A half-open UTF-8 byte range in the source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

/// Stable diagnostic categories exposed to editor integrations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub range: ByteRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Key,
    Table,
    ArrayOfTables,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: ByteRange,
    pub children: Vec<Symbol>,
}

/// Result of analyzing one immutable source snapshot.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Analysis {
    pub diagnostics: Vec<Diagnostic>,
    pub symbols: Vec<Symbol>,
    pub semantic: Option<SemanticIndex>,
}

/// Parse and index a complete ATML document snapshot.
pub fn analyze(source: &str) -> Analysis {
    match Document::parse_atml(source) {
        Ok(document) => analyze_document(source, &document),
        Err(error)
            if matches!(
                error.kind,
                TomlErrorKind::KeyNotFound(_) | TomlErrorKind::CyclicPathRef(_)
            ) =>
        {
            diagnostics::recover_reference_document(source).map_or_else(
                || recover_valid_prefix(source, &error),
                |document| analyze_document(source, &document),
            )
        }
        Err(error) => recover_valid_prefix(source, &error),
    }
}

fn recover_valid_prefix(source: &str, error: &TomlError) -> Analysis {
    let syntax_diagnostic = diagnostic_from_error(source, error);
    let mut boundary = source[..syntax_diagnostic.range.start.min(source.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    loop {
        let prefix = &source[..boundary];
        if let Ok(document) = Document::parse_atml(prefix) {
            let mut analysis = analyze_document(prefix, &document);
            analysis.diagnostics.push(syntax_diagnostic);
            analysis
                .diagnostics
                .sort_by_key(|diagnostic| (diagnostic.range.start, diagnostic.code));
            return analysis;
        }
        if boundary == 0 {
            return Analysis {
                diagnostics: vec![syntax_diagnostic],
                symbols: Vec::new(),
                semantic: Some(SemanticIndex::default()),
            };
        }
        boundary = source[..boundary.saturating_sub(1)]
            .rfind('\n')
            .map_or(0, |index| index + 1);
    }
}

fn analyze_document(source: &str, document: &Document) -> Analysis {
    let semantic = SemanticIndex::build(source, document);
    Analysis {
        diagnostics: diagnostics::semantic_diagnostics(&semantic),
        symbols: collect_symbols(source, document),
        semantic: Some(semantic),
    }
}

fn diagnostic_from_error(source: &str, error: &TomlError) -> Diagnostic {
    let offset = error
        .location
        .as_ref()
        .map(|location| line_column_to_byte(source, location.line, location.column))
        .unwrap_or(0);
    let end = next_char_boundary(source, offset);

    Diagnostic {
        code: match error.kind {
            TomlErrorKind::DuplicateKey => "toml.duplicate-key",
            TomlErrorKind::IntegerOverflow => "toml.integer-overflow",
            TomlErrorKind::InvalidEscape(_) => "toml.invalid-escape",
            _ => "atml.syntax.parse-error",
        },
        severity: DiagnosticSeverity::Error,
        message: error.message.clone(),
        range: ByteRange { start: offset, end },
    }
}

fn collect_symbols(source: &str, document: &Document) -> Vec<Symbol> {
    struct IndexedSymbol {
        symbol: Symbol,
    }

    fn node_mut<'a>(nodes: &'a mut [IndexedSymbol], indices: &[usize]) -> &'a mut Symbol {
        let node = &mut nodes[indices[0]];
        if indices.len() == 1 {
            &mut node.symbol
        } else {
            symbol_mut(&mut node.symbol.children, &indices[1..])
        }
    }

    fn symbol_mut<'a>(nodes: &'a mut [Symbol], indices: &[usize]) -> &'a mut Symbol {
        let node = &mut nodes[indices[0]];
        if indices.len() == 1 {
            node
        } else {
            symbol_mut(&mut node.children, &indices[1..])
        }
    }

    let mut roots: Vec<IndexedSymbol> = Vec::new();
    let mut cursor = 0;
    let mut current_section: Option<Vec<usize>> = None;
    let mut known_sections: Vec<(Vec<String>, Vec<usize>)> = Vec::new();

    for item in document.items() {
        let (needle, semantic_path, kind) = match item {
            DocumentItem::Entry { node, path } => {
                (node.raw_key.as_str(), path.clone(), SymbolKind::Key)
            }
            DocumentItem::Section(section) => (
                section.raw.as_str(),
                section.path.clone(),
                if section.is_array {
                    SymbolKind::ArrayOfTables
                } else {
                    SymbolKind::Table
                },
            ),
            DocumentItem::Eof(_) => continue,
        };

        if let Some(relative) = source[cursor..].find(needle) {
            let start = cursor + relative;
            let end = start + needle.len();
            let parent = if kind == SymbolKind::Key {
                current_section.clone()
            } else {
                known_sections
                    .iter()
                    .filter(|(candidate, _)| {
                        candidate.len() < semantic_path.len()
                            && semantic_path.starts_with(candidate)
                    })
                    .max_by_key(|(candidate, _)| candidate.len())
                    .map(|(_, indices)| indices.clone())
            };
            let name = if kind == SymbolKind::Key {
                needle.trim().trim_end_matches("[]").to_owned()
            } else if parent.is_some() {
                semantic_path.last().cloned().unwrap_or_default()
            } else {
                semantic_path.join(".")
            };
            let symbol = Symbol {
                name,
                kind,
                range: ByteRange { start, end },
                children: Vec::new(),
            };

            if let Some(indices) = parent {
                let parent = node_mut(&mut roots, &indices);
                parent.children.push(symbol);
                if kind != SymbolKind::Key {
                    let mut child_indices = indices;
                    child_indices.push(parent.children.len() - 1);
                    current_section = Some(child_indices.clone());
                    known_sections.push((semantic_path, child_indices));
                }
            } else {
                roots.push(IndexedSymbol { symbol });
                if kind != SymbolKind::Key {
                    let indices = vec![roots.len() - 1];
                    current_section = Some(indices.clone());
                    known_sections.push((semantic_path, indices));
                }
            }
            cursor = end;
        }
    }

    roots.into_iter().map(|node| node.symbol).collect()
}

fn line_column_to_byte(source: &str, line: u32, column: u32) -> usize {
    let target_line = line.saturating_sub(1) as usize;
    let target_column = column.saturating_sub(1) as usize;
    let line_start = source
        .split_inclusive('\n')
        .take(target_line)
        .map(str::len)
        .sum::<usize>();
    let line_text = source[line_start..].split('\n').next().unwrap_or_default();
    line_start
        + line_text
            .char_indices()
            .map(|(offset, _)| offset)
            .nth(target_column)
            .unwrap_or(line_text.len())
}

fn next_char_boundary(source: &str, offset: usize) -> usize {
    source
        .get(offset..)
        .and_then(|tail| tail.chars().next().map(char::len_utf8))
        .map_or(offset, |width| offset + width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_atml_has_symbols_and_no_diagnostics() {
        let result = analyze(
            "Strategy[] = [Active, Passive]\n\
             [defaults]\n\
             timeout = 500ms\n\
             [server : defaults]\n\
             mode = Strategy::Active\n",
        );

        assert!(result.diagnostics.is_empty());
        assert!(result
            .symbols
            .iter()
            .any(|symbol| symbol.name == "Strategy"));
        assert!(result.symbols.iter().any(|symbol| symbol.name == "server"));
    }

    #[test]
    fn invalid_atml_has_stable_diagnostic_code() {
        let result = analyze("value = 12^\n");

        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, "atml.syntax.parse-error");
        assert!(result.diagnostics[0].range.start <= result.diagnostics[0].range.end);
    }

    #[test]
    fn unicode_before_error_produces_a_character_boundary() {
        let source = "label = \"Größe\"\nbroken = [\n";
        let result = analyze(source);
        let range = result.diagnostics[0].range;

        assert!(source.is_char_boundary(range.start));
        assert!(source.is_char_boundary(range.end));
    }

    #[test]
    fn tables_contain_keys_and_subtables() {
        let result = analyze(
            "root_key = 1\n\
             [server]\n\
             host = \"localhost\"\n\
             [server.tls]\n\
             enabled = true\n",
        );

        let server = result
            .symbols
            .iter()
            .find(|symbol| symbol.name == "server")
            .unwrap();
        assert!(server.children.iter().any(|symbol| symbol.name == "host"));
        let tls = server
            .children
            .iter()
            .find(|symbol| symbol.name == "tls")
            .unwrap();
        assert!(tls.children.iter().any(|symbol| symbol.name == "enabled"));
    }
}
