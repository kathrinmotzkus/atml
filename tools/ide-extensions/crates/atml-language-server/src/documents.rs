use std::collections::HashMap;

use lsp_types::{Position, Range, TextDocumentContentChangeEvent, Uri};

#[derive(Debug, Clone)]
pub struct OpenDocument {
    pub version: i32,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Documents {
    open: HashMap<Uri, OpenDocument>,
}

impl Documents {
    pub fn open(&mut self, uri: Uri, version: i32, text: String) {
        self.open.insert(uri, OpenDocument { version, text });
    }

    pub fn change(
        &mut self,
        uri: &Uri,
        version: i32,
        changes: Vec<TextDocumentContentChangeEvent>,
    ) -> Option<&OpenDocument> {
        let document = self.open.get_mut(uri)?;
        if version <= document.version {
            return None;
        }

        for change in changes {
            if let Some(range) = change.range {
                if let Some((start, end)) = range_to_bytes(&document.text, range) {
                    document.text.replace_range(start..end, &change.text);
                }
            } else {
                document.text = change.text;
            }
        }
        document.version = version;
        Some(document)
    }

    pub fn close(&mut self, uri: &Uri) {
        self.open.remove(uri);
    }

    pub fn get(&self, uri: &Uri) -> Option<&OpenDocument> {
        self.open.get(uri)
    }
}

pub fn byte_to_position(text: &str, byte_offset: usize) -> Position {
    let offset = byte_offset.min(text.len());
    let prefix = &text[..floor_char_boundary(text, offset)];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let character = prefix[line_start..].encode_utf16().count() as u32;
    Position::new(line, character)
}

fn range_to_bytes(text: &str, range: Range) -> Option<(usize, usize)> {
    let start = position_to_byte(text, range.start)?;
    let end = position_to_byte(text, range.end)?;
    (start <= end).then_some((start, end))
}

fn position_to_byte(text: &str, position: Position) -> Option<usize> {
    let line_start = if position.line == 0 {
        0
    } else {
        text.match_indices('\n')
            .nth(position.line as usize - 1)
            .map(|(index, _)| index + 1)?
    };
    let line = text[line_start..].split('\n').next().unwrap_or_default();
    let mut utf16_column = 0_u32;

    for (byte, character) in line.char_indices() {
        if utf16_column == position.character {
            return Some(line_start + byte);
        }
        utf16_column += character.len_utf16() as u32;
        if utf16_column > position.character {
            return None;
        }
    }

    (utf16_column == position.character).then_some(line_start + line.len())
}

fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_use_utf16_columns() {
        let text = "a😀b\nnext";
        assert_eq!(byte_to_position(text, "a😀".len()), Position::new(0, 3));
    }

    #[test]
    fn incremental_change_replaces_utf16_range() {
        let uri: Uri = "file:///test.atml".parse().unwrap();
        let mut documents = Documents::default();
        documents.open(uri.clone(), 1, "name = \"😀\"\n".into());
        documents.change(
            &uri,
            2,
            vec![TextDocumentContentChangeEvent {
                range: Some(Range::new(Position::new(0, 8), Position::new(0, 10))),
                range_length: None,
                text: "ATML".into(),
            }],
        );

        assert_eq!(documents.get(&uri).unwrap().text, "name = \"ATML\"\n");
    }

    #[test]
    fn incremental_changes_handle_ascii_umlauts_and_non_bmp_characters() {
        let uri: Uri = "file:///unicode.atml".parse().unwrap();
        let mut documents = Documents::default();
        documents.open(uri.clone(), 1, "value = \"Aä😀Z\"\n".into());

        let cases = [
            (2, Position::new(0, 9), Position::new(0, 10), "B"),
            (3, Position::new(0, 10), Position::new(0, 11), "ö"),
            (4, Position::new(0, 11), Position::new(0, 13), "🎉"),
        ];
        for (version, start, end, replacement) in cases {
            documents.change(
                &uri,
                version,
                vec![TextDocumentContentChangeEvent {
                    range: Some(Range::new(start, end)),
                    range_length: None,
                    text: replacement.into(),
                }],
            );
        }

        assert_eq!(documents.get(&uri).unwrap().text, "value = \"Bö🎉Z\"\n");
    }

    #[test]
    fn stale_document_versions_are_ignored() {
        let uri: Uri = "file:///versioned.atml".parse().unwrap();
        let mut documents = Documents::default();
        documents.open(uri.clone(), 5, "value = 5\n".into());

        let result = documents.change(
            &uri,
            4,
            vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "value = 4\n".into(),
            }],
        );

        assert!(result.is_none());
        assert_eq!(documents.get(&uri).unwrap().text, "value = 5\n");
    }
}
