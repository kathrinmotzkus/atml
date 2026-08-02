use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use serde_json::{json, Value};

const TIMEOUT: Duration = Duration::from_secs(10);

struct ChildGuard(std::process::Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn complete_lsp_session_updates_diagnostics_and_symbols() {
    let child = Command::new(env!("CARGO_BIN_EXE_atml-language-server"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("start language server");
    let mut child = ChildGuard(child);
    let mut input = child.0.stdin.take().expect("server stdin");
    let output = child.0.stdout.take().expect("server stdout");
    let messages = read_messages(output);

    send(
        &mut input,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "capabilities": {} }
        }),
    );
    let initialized = receive(&messages);
    assert_eq!(initialized["id"], 1);
    assert_eq!(
        initialized["result"]["capabilities"]["textDocumentSync"], 2,
        "{initialized:#}"
    );
    send(
        &mut input,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );

    let uri = "file:///integration.atml";
    send(
        &mut input,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "atml",
                    "version": 1,
                    "text": "broken = [\n"
                }
            }
        }),
    );
    let invalid = receive_method(&messages, "textDocument/publishDiagnostics");
    assert_eq!(invalid["params"]["version"], 1);
    assert_eq!(
        invalid["params"]["diagnostics"][0]["code"],
        "atml.syntax.parse-error"
    );

    let valid = "[server]\nhost = \"localhost\"\n[server.tls]\nenabled = true\n";
    send(
        &mut input,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{ "text": valid }]
            }
        }),
    );
    send(
        &mut input,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/documentSymbol",
            "params": { "textDocument": { "uri": uri } }
        }),
    );
    let mut symbols = None;
    let mut valid_diagnostics = None;
    while symbols.is_none() || valid_diagnostics.is_none() {
        let message = receive(&messages);
        if message["id"] == 2 {
            symbols = Some(message);
        } else if message["method"] == "textDocument/publishDiagnostics"
            && message["params"]["version"] == 2
        {
            valid_diagnostics = Some(message);
        }
    }
    let symbols = symbols.unwrap();
    assert_eq!(symbols["result"][0]["name"], "server");
    assert_eq!(symbols["result"][0]["children"][0]["name"], "host");
    assert_eq!(symbols["result"][0]["children"][1]["name"], "tls");

    let valid_diagnostics = valid_diagnostics.unwrap();
    assert_eq!(valid_diagnostics["params"]["version"], 2);
    assert_eq!(valid_diagnostics["params"]["diagnostics"], json!([]));

    // Version 3 is superseded before its debounce window closes. The server
    // must publish only the current version 4 analysis.
    for (version, text) in [(3, "broken = [\n"), (4, "value = 4\n")] {
        send(
            &mut input,
            json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didChange",
                "params": {
                    "textDocument": { "uri": uri, "version": version },
                    "contentChanges": [{ "text": text }]
                }
            }),
        );
    }
    let newest = receive_method(&messages, "textDocument/publishDiagnostics");
    assert_eq!(newest["params"]["version"], 4);
    assert_eq!(newest["params"]["diagnostics"], json!([]));

    send(
        &mut input,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": 5 },
                "contentChanges": [{
                    "text": "Mode[] = [Active]\nbad = Mode::Missing\nfirst = missing.one\nsecond = other.value\n"
                }]
            }
        }),
    );
    let semantic = receive_method(&messages, "textDocument/publishDiagnostics");
    assert_eq!(semantic["params"]["version"], 5);
    let codes = semantic["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().unwrap())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        codes,
        std::collections::HashSet::from([
            "atml.enum.unknown-member",
            "atml.reference.unknown-target",
        ])
    );
    assert_eq!(
        semantic["params"]["diagnostics"].as_array().unwrap().len(),
        3
    );

    send(
        &mut input,
        json!({"jsonrpc":"2.0","id":99,"method":"shutdown","params":null}),
    );
    assert_eq!(receive_id(&messages, 99)["result"], Value::Null);
    send(
        &mut input,
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    );
    drop(input);
    assert!(child.0.wait().expect("wait for server").success());
}

fn send(input: &mut ChildStdin, message: Value) {
    let body = serde_json::to_vec(&message).unwrap();
    write!(input, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
    input.write_all(&body).unwrap();
    input.flush().unwrap();
}

fn read_messages(output: impl Read + Send + 'static) -> Receiver<Value> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(output);
        loop {
            let mut content_length = None;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 {
                    return;
                }
                if header == "\r\n" {
                    break;
                }
                if let Some(value) = header.strip_prefix("Content-Length:") {
                    content_length = value.trim().parse::<usize>().ok();
                }
            }
            let Some(length) = content_length else {
                return;
            };
            let mut body = vec![0; length];
            if reader.read_exact(&mut body).is_err() {
                return;
            }
            if let Ok(message) = serde_json::from_slice(&body) {
                if sender.send(message).is_err() {
                    return;
                }
            }
        }
    });
    receiver
}

fn receive(messages: &Receiver<Value>) -> Value {
    messages.recv_timeout(TIMEOUT).expect("LSP response")
}

fn receive_method(messages: &Receiver<Value>, method: &str) -> Value {
    loop {
        let message = receive(messages);
        if message["method"] == method {
            return message;
        }
    }
}

fn receive_id(messages: &Receiver<Value>, id: i64) -> Value {
    loop {
        let message = receive(messages);
        if message["id"] == id {
            return message;
        }
    }
}
