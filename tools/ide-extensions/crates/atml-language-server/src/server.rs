use std::{
    collections::HashMap,
    error::Error,
    time::{Duration, Instant},
};

use atml_language_core::{
    complete, find_references, goto_definition, hover, CompletionKind as CoreCompletionKind,
    DiagnosticSeverity as CoreDiagnosticSeverity, SymbolKind,
};
use crossbeam_channel::RecvTimeoutError;
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    CompletionItem as LspCompletionItem, CompletionItemKind as LspCompletionItemKind,
    CompletionOptions, CompletionParams, CompletionResponse, CompletionTextEdit,
    Diagnostic as LspDiagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentSymbol, DocumentSymbolParams,
    DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents,
    HoverParams, InitializeResult, Location, MarkupContent, MarkupKind, OneOf,
    PublishDiagnosticsParams, ReferenceParams, ServerCapabilities, ServerInfo,
    SymbolKind as LspSymbolKind, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit,
};

use crate::documents::{byte_to_position, Documents, OpenDocument};

const DOCUMENT_SYMBOL_METHOD: &str = "textDocument/documentSymbol";
const COMPLETION_METHOD: &str = "textDocument/completion";
const HOVER_METHOD: &str = "textDocument/hover";
const DEFINITION_METHOD: &str = "textDocument/definition";
const REFERENCES_METHOD: &str = "textDocument/references";
const DID_OPEN_METHOD: &str = "textDocument/didOpen";
const DID_CHANGE_METHOD: &str = "textDocument/didChange";
const DID_CLOSE_METHOD: &str = "textDocument/didClose";
const PUBLISH_DIAGNOSTICS_METHOD: &str = "textDocument/publishDiagnostics";
const ANALYSIS_DEBOUNCE: Duration = Duration::from_millis(75);
const EVENT_LOOP_TICK: Duration = Duration::from_millis(20);

struct PendingAnalysis {
    uri: lsp_types::Uri,
    version: i32,
    due: Instant,
}

pub fn run() -> Result<(), Box<dyn Error + Send + Sync>> {
    let (connection, io_threads) = Connection::stdio();
    eprintln!(
        "ATML language server {} starting",
        env!("CARGO_PKG_VERSION")
    );
    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::INCREMENTAL,
        )),
        document_symbol_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![":".into(), ".".into(), "/".into(), "*".into()]),
            ..CompletionOptions::default()
        }),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        ..ServerCapabilities::default()
    };
    let initialize_result = InitializeResult {
        capabilities,
        server_info: Some(ServerInfo {
            name: "ATML Language Server".into(),
            version: Some(env!("CARGO_PKG_VERSION").into()),
        }),
    };
    let (initialize_id, _) = connection.initialize_start()?;
    connection.initialize_finish(initialize_id, serde_json::to_value(initialize_result)?)?;

    let mut documents = Documents::default();
    let mut pending = HashMap::new();
    loop {
        match connection.receiver.recv_timeout(EVENT_LOOP_TICK) {
            Ok(message) => match message {
                Message::Request(request) => {
                    if connection.handle_shutdown(&request)? {
                        break;
                    }
                    handle_request(&connection, &mut documents, request)?;
                }
                Message::Notification(notification) => {
                    handle_notification(&connection, &mut documents, &mut pending, notification)?
                }
                Message::Response(_) => {}
            },
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        publish_due_analyses(&connection, &mut documents, &mut pending)?;
    }

    drop(connection);
    io_threads.join()?;
    eprintln!("ATML language server stopped");
    Ok(())
}

fn handle_request(
    connection: &Connection,
    documents: &mut Documents,
    request: Request,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if request.method == HOVER_METHOD {
        let id = request.id.clone();
        let params: HoverParams = serde_json::from_value(request.params)?;
        let result = documents
            .get_mut(&params.text_document_position_params.text_document.uri)
            .and_then(|document| {
                let offset = crate::documents::position_to_byte(
                    &document.text,
                    params.text_document_position_params.position,
                )?;
                let index = document.analysis().semantic.clone()?;
                let result = hover(&document.text, &index, offset)?;
                Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: result.markdown,
                    }),
                    range: Some(core_range_to_lsp(&document.text, result.range)),
                })
            });
        connection.sender.send(Message::Response(Response::new_ok(
            id,
            serde_json::to_value(result)?,
        )))?;
        return Ok(());
    }
    if request.method == DEFINITION_METHOD {
        let id = request.id.clone();
        let params: GotoDefinitionParams = serde_json::from_value(request.params)?;
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let result = documents.get_mut(&uri).and_then(|document| {
            let offset = crate::documents::position_to_byte(
                &document.text,
                params.text_document_position_params.position,
            )?;
            let index = document.analysis().semantic.clone()?;
            let target = goto_definition(&index, offset)?;
            Some(GotoDefinitionResponse::Scalar(Location::new(
                uri.clone(),
                core_range_to_lsp(&document.text, target.selection_range),
            )))
        });
        connection.sender.send(Message::Response(Response::new_ok(
            id,
            serde_json::to_value(result)?,
        )))?;
        return Ok(());
    }
    if request.method == REFERENCES_METHOD {
        let id = request.id.clone();
        let params: ReferenceParams = serde_json::from_value(request.params)?;
        let uri = params.text_document_position.text_document.uri.clone();
        let result = documents
            .get_mut(&uri)
            .and_then(|document| {
                let offset = crate::documents::position_to_byte(
                    &document.text,
                    params.text_document_position.position,
                )?;
                let index = document.analysis().semantic.clone()?;
                Some(
                    find_references(&index, offset, params.context.include_declaration)
                        .into_iter()
                        .map(|range| {
                            Location::new(uri.clone(), core_range_to_lsp(&document.text, range))
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap_or_default();
        connection.sender.send(Message::Response(Response::new_ok(
            id,
            serde_json::to_value(result)?,
        )))?;
        return Ok(());
    }
    if request.method == COMPLETION_METHOD {
        let id = request.id.clone();
        let params: CompletionParams = serde_json::from_value(request.params)?;
        let result = documents
            .get_mut(&params.text_document_position.text_document.uri)
            .and_then(|document| {
                let offset = crate::documents::position_to_byte(
                    &document.text,
                    params.text_document_position.position,
                )?;
                Some(completion_items(document, offset))
            })
            .unwrap_or_default();
        connection.sender.send(Message::Response(Response::new_ok(
            id,
            serde_json::to_value(CompletionResponse::Array(result))?,
        )))?;
        return Ok(());
    }
    if request.method != DOCUMENT_SYMBOL_METHOD {
        connection.sender.send(Message::Response(Response::new_err(
            request.id,
            lsp_server::ErrorCode::MethodNotFound as i32,
            format!("unsupported request: {}", request.method),
        )))?;
        return Ok(());
    }

    let id = request.id.clone();
    let params: DocumentSymbolParams = serde_json::from_value(request.params)?;
    let result = documents
        .get_mut(&params.text_document.uri)
        .map(document_symbols)
        .unwrap_or_default();
    let response = Response::new_ok(
        id,
        serde_json::to_value(DocumentSymbolResponse::Nested(result))?,
    );
    connection.sender.send(Message::Response(response))?;
    Ok(())
}

fn completion_items(document: &OpenDocument, offset: usize) -> Vec<LspCompletionItem> {
    complete(&document.text, offset)
        .into_iter()
        .map(|item| LspCompletionItem {
            label: item.label,
            kind: Some(match item.kind {
                CoreCompletionKind::Enum => LspCompletionItemKind::ENUM,
                CoreCompletionKind::EnumMember => LspCompletionItemKind::ENUM_MEMBER,
                CoreCompletionKind::Key => LspCompletionItemKind::REFERENCE,
                CoreCompletionKind::Table => LspCompletionItemKind::STRUCT,
                CoreCompletionKind::Unit => LspCompletionItemKind::UNIT,
                CoreCompletionKind::Value => LspCompletionItemKind::VALUE,
            }),
            detail: Some(item.detail),
            sort_text: Some(item.sort_text),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: lsp_types::Range::new(
                    byte_to_position(&document.text, item.replace.start),
                    byte_to_position(&document.text, item.replace.end),
                ),
                new_text: item.insert_text,
            })),
            ..LspCompletionItem::default()
        })
        .collect()
}

fn core_range_to_lsp(text: &str, range: atml_language_core::ByteRange) -> lsp_types::Range {
    lsp_types::Range::new(
        byte_to_position(text, range.start),
        byte_to_position(text, range.end),
    )
}

fn handle_notification(
    connection: &Connection,
    documents: &mut Documents,
    pending: &mut HashMap<String, PendingAnalysis>,
    notification: Notification,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match notification.method.as_str() {
        DID_OPEN_METHOD => {
            let params: DidOpenTextDocumentParams = serde_json::from_value(notification.params)?;
            let document = params.text_document;
            eprintln!(
                "opened {} at version {}",
                document.uri.as_str(),
                document.version
            );
            documents.open(document.uri.clone(), document.version, document.text);
            schedule_analysis(pending, document.uri, document.version);
        }
        DID_CHANGE_METHOD => {
            let params: DidChangeTextDocumentParams = serde_json::from_value(notification.params)?;
            let uri = params.text_document.uri;
            if documents
                .change(&uri, params.text_document.version, params.content_changes)
                .is_some()
            {
                schedule_analysis(pending, uri, params.text_document.version);
            }
        }
        DID_CLOSE_METHOD => {
            let params: DidCloseTextDocumentParams = serde_json::from_value(notification.params)?;
            documents.close(&params.text_document.uri);
            pending.remove(params.text_document.uri.as_str());
            send_diagnostics(connection, params.text_document.uri, None, Vec::new())?;
        }
        _ => {}
    }
    Ok(())
}

fn schedule_analysis(
    pending: &mut HashMap<String, PendingAnalysis>,
    uri: lsp_types::Uri,
    version: i32,
) {
    pending.insert(
        uri.as_str().to_owned(),
        PendingAnalysis {
            uri,
            version,
            due: Instant::now() + ANALYSIS_DEBOUNCE,
        },
    );
}

fn publish_due_analyses(
    connection: &Connection,
    documents: &mut Documents,
    pending: &mut HashMap<String, PendingAnalysis>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let now = Instant::now();
    let due = pending
        .iter()
        .filter(|(_, analysis)| analysis.due <= now)
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();

    for key in due {
        if let Some(analysis) = pending.remove(&key) {
            if let Some(document) = documents.get_mut(&analysis.uri) {
                if document.version == analysis.version {
                    eprintln!(
                        "analyzing {} at version {}",
                        analysis.uri.as_str(),
                        analysis.version
                    );
                    publish_diagnostics(connection, analysis.uri, document)?;
                }
            }
        }
    }
    Ok(())
}

fn publish_diagnostics(
    connection: &Connection,
    uri: lsp_types::Uri,
    document: &mut OpenDocument,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let core_diagnostics = document.analysis().diagnostics.clone();
    let diagnostics = core_diagnostics
        .iter()
        .map(|diagnostic| LspDiagnostic {
            range: lsp_types::Range::new(
                byte_to_position(&document.text, diagnostic.range.start),
                byte_to_position(&document.text, diagnostic.range.end),
            ),
            severity: Some(match diagnostic.severity {
                CoreDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
                CoreDiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
            }),
            code: Some(lsp_types::NumberOrString::String(diagnostic.code.into())),
            source: Some("atml".into()),
            message: diagnostic.message.clone(),
            ..LspDiagnostic::default()
        })
        .collect();
    send_diagnostics(connection, uri, Some(document.version), diagnostics)
}

fn send_diagnostics(
    connection: &Connection,
    uri: lsp_types::Uri,
    version: Option<i32>,
    diagnostics: Vec<LspDiagnostic>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let params = PublishDiagnosticsParams::new(uri, diagnostics, version);
    connection
        .sender
        .send(Message::Notification(Notification::new(
            PUBLISH_DIAGNOSTICS_METHOD.into(),
            params,
        )))?;
    Ok(())
}

fn document_symbols(document: &mut OpenDocument) -> Vec<DocumentSymbol> {
    fn convert(document: &OpenDocument, symbol: atml_language_core::Symbol) -> DocumentSymbol {
        let range = lsp_types::Range::new(
            byte_to_position(&document.text, symbol.range.start),
            byte_to_position(&document.text, symbol.range.end),
        );
        let children = symbol
            .children
            .into_iter()
            .map(|child| convert(document, child))
            .collect::<Vec<_>>();
        #[allow(deprecated)]
        DocumentSymbol {
            name: symbol.name,
            detail: None,
            kind: match symbol.kind {
                SymbolKind::Key => LspSymbolKind::FIELD,
                SymbolKind::Table => LspSymbolKind::OBJECT,
                SymbolKind::ArrayOfTables => LspSymbolKind::ARRAY,
            },
            tags: None,
            deprecated: None,
            range,
            selection_range: range,
            children: (!children.is_empty()).then_some(children),
        }
    }
    document
        .analysis()
        .symbols
        .clone()
        .into_iter()
        .map(|symbol| convert(document, symbol))
        .collect()
}
