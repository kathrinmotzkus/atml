//! Editor-independent analysis for ATML documents.

use toml_dom::{Document, DocumentItem, TomlError};

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
    pub message: String,
    pub range: ByteRange,
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
}

/// Result of analyzing one immutable source snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Analysis {
    pub diagnostics: Vec<Diagnostic>,
    pub symbols: Vec<Symbol>,
}

/// Parse and index a complete ATML document snapshot.
pub fn analyze(source: &str) -> Analysis {
    match Document::parse_atml(source) {
        Ok(document) => Analysis {
            diagnostics: Vec::new(),
            symbols: collect_symbols(source, &document),
        },
        Err(error) => Analysis {
            diagnostics: vec![diagnostic_from_error(source, &error)],
            symbols: Vec::new(),
        },
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
        code: "atml.syntax.parse-error",
        message: error.message.clone(),
        range: ByteRange { start: offset, end },
    }
}

fn collect_symbols(source: &str, document: &Document) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let mut cursor = 0;

    for item in document.items() {
        let (needle, name, kind) = match item {
            DocumentItem::Entry { node, path } => {
                (node.raw_key.as_str(), path.join("."), SymbolKind::Key)
            }
            DocumentItem::Section(section) => (
                section.raw.as_str(),
                section.path.join("."),
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
            symbols.push(Symbol {
                name,
                kind,
                range: ByteRange { start, end },
            });
            cursor = end;
        }
    }

    symbols
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
}
