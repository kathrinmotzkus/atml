use std::error::Error;

use atml_language_core::{analyze, Analysis, SymbolKind};
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

pub fn run() -> Result<(), Box<dyn Error + Send + Sync>> {
    let (connection, io_threads) = Connection::stdio();
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
    connection.initialize(serde_json::to_value(initialize_result)?)?;

    let mut documents = Documents::default();
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    break;
                }
                handle_request(&connection, &documents, request)?;
            }
            Message::Notification(notification) => {
                handle_notification(&connection, &mut documents, notification)?;
            }
            Message::Response(_) => {}
        }
    }

    io_threads.join()?;
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
    notification: Notification,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match notification.method.as_str() {
        DID_OPEN_METHOD => {
            let params: DidOpenTextDocumentParams = serde_json::from_value(notification.params)?;
            let document = params.text_document;
            documents.open(document.uri.clone(), document.version, document.text);
            if let Some(open) = documents.get(&document.uri) {
                publish_diagnostics(connection, document.uri, open)?;
            }
        }
        DID_CHANGE_METHOD => {
            let params: DidChangeTextDocumentParams = serde_json::from_value(notification.params)?;
            let uri = params.text_document.uri;
            if let Some(open) =
                documents.change(&uri, params.text_document.version, params.content_changes)
            {
                publish_diagnostics(connection, uri, open)?;
            }
        }
        DID_CLOSE_METHOD => {
            let params: DidCloseTextDocumentParams = serde_json::from_value(notification.params)?;
            documents.close(&params.text_document.uri);
            send_diagnostics(connection, params.text_document.uri, None, Vec::new())?;
        }
        _ => {}
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
    symbols
        .into_iter()
        .map(|symbol| {
            let range = lsp_types::Range::new(
                byte_to_position(&document.text, symbol.range.start),
                byte_to_position(&document.text, symbol.range.end),
            );
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
                children: None,
            }
        })
        .collect()
}
