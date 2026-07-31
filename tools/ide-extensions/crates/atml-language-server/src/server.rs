use std::{
    collections::HashMap,
    error::Error,
    time::{Duration, Instant},
};

use atml_language_core::{analyze, Analysis, SymbolKind};
use crossbeam_channel::RecvTimeoutError;
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentSymbol, DocumentSymbolParams,
    DocumentSymbolResponse, InitializeResult, OneOf, PublishDiagnosticsParams, ServerCapabilities,
    ServerInfo, SymbolKind as LspSymbolKind, TextDocumentSyncCapability, TextDocumentSyncKind,
};

use crate::documents::{byte_to_position, Documents, OpenDocument};

const DOCUMENT_SYMBOL_METHOD: &str = "textDocument/documentSymbol";
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
                    handle_request(&connection, &documents, request)?;
                }
                Message::Notification(notification) => {
                    handle_notification(&connection, &mut documents, &mut pending, notification)?
                }
                Message::Response(_) => {}
            },
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        publish_due_analyses(&connection, &documents, &mut pending)?;
    }

    drop(connection);
    io_threads.join()?;
    eprintln!("ATML language server stopped");
    Ok(())
}

fn handle_request(
    connection: &Connection,
    documents: &Documents,
    request: Request,
) -> Result<(), Box<dyn Error + Send + Sync>> {
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
        .get(&params.text_document.uri)
        .map(document_symbols)
        .unwrap_or_default();
    let response = Response::new_ok(
        id,
        serde_json::to_value(DocumentSymbolResponse::Nested(result))?,
    );
    connection.sender.send(Message::Response(response))?;
    Ok(())
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
    documents: &Documents,
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
            if let Some(document) = documents.get(&analysis.uri) {
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
    document: &OpenDocument,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let analysis = analyze(&document.text);
    let diagnostics = analysis
        .diagnostics
        .into_iter()
        .map(|diagnostic| LspDiagnostic {
            range: lsp_types::Range::new(
                byte_to_position(&document.text, diagnostic.range.start),
                byte_to_position(&document.text, diagnostic.range.end),
            ),
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(lsp_types::NumberOrString::String(diagnostic.code.into())),
            source: Some("atml".into()),
            message: diagnostic.message,
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

fn document_symbols(document: &OpenDocument) -> Vec<DocumentSymbol> {
    let Analysis { symbols, .. } = analyze(&document.text);
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
    symbols
        .into_iter()
        .map(|symbol| convert(document, symbol))
        .collect()
}
