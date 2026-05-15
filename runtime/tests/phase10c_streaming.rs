//! Phase 10C - SSE streaming.
//!
//! Uses local mock upstreams in place of llama-server. Verifies:
//!   1. A multi-chunk SSE response is reassembled correctly.
//!   2. Lines that span chunk boundaries are not truncated.
//!   3. The terminating `data: [DONE]` is forwarded.
//!   4. Empty/comment lines are skipped.
//!   5. Upstream connection error surfaces as an SSE error event.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use httpmock::{Method::POST, MockServer};
use rusqlite::Connection;
use serde_json::{json, Value};
use skycode_api::server::run;
use skycode_api::state::AppState;
use tempfile::TempDir;

#[test]
fn phase10c_streaming_multi_chunk_reassembly() -> Result<(), Box<dyn std::error::Error>> {
    let event = concat!(
        "data: {\"id\":\"x\",\"choices\":[{\"delta\":{\"content\":\"hello world\"},",
        "\"index\":0,\"finish_reason\":null}]}\n\n"
    );
    let split_at = event
        .find("world")
        .ok_or("test event must contain split marker")?;
    let upstream = ScriptedUpstream::complete(vec![
        event.as_bytes()[..split_at].to_vec(),
        event.as_bytes()[split_at..].to_vec(),
        b"data: [DONE]\n\n".to_vec(),
    ])?;
    let api = TestApi::start(upstream.port)?;

    let response = post_chat(
        api.port,
        &api.api_key,
        &json!({
            "model": "local-coder",
            "stream": true,
            "messages": [{"role": "user", "content": "count"}]
        })
        .to_string(),
    )?;
    let data = sse_data_lines(&response.body);

    assert_eq!(data.len(), 2);
    assert_eq!(content_from_data(&data[0])?, "hello world");
    assert_eq!(data[1], "[DONE]");

    Ok(())
}

#[test]
fn phase10c_streaming_terminator_forwarded() -> Result<(), Box<dyn std::error::Error>> {
    let upstream = ScriptedUpstream::complete(vec![b"data: [DONE]\n\n".to_vec()])?;
    let api = TestApi::start(upstream.port)?;

    let response = post_chat(api.port, &api.api_key, &stream_request_body())?;

    assert!(
        response.body.trim_end().ends_with("data: [DONE]"),
        "SSE body must end with [DONE], got: {}",
        response.body
    );

    Ok(())
}

#[test]
fn phase10c_streaming_comments_skipped() -> Result<(), Box<dyn std::error::Error>> {
    let upstream = ScriptedUpstream::complete(vec![
        b":keepalive\n\n".to_vec(),
        b"event: message\nid: 1\nretry: 1000\n\n".to_vec(),
        b"data: {\"id\":\"a\",\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\n".to_vec(),
        b"data: [DONE]\n\n".to_vec(),
    ])?;
    let api = TestApi::start(upstream.port)?;

    let response = post_chat(api.port, &api.api_key, &stream_request_body())?;
    let data = sse_data_lines(&response.body);

    assert_eq!(data.len(), 2);
    assert_eq!(content_from_data(&data[0])?, "ok");
    assert_eq!(data[1], "[DONE]");
    assert!(!response.body.contains(":keepalive"));
    assert!(!response.body.contains("event:"));
    assert!(!response.body.contains("retry:"));

    Ok(())
}

#[test]
fn phase10c_streaming_upstream_error_event() -> Result<(), Box<dyn std::error::Error>> {
    let upstream = ScriptedUpstream::broken_after(
        vec![
            b"data: {\"id\":\"before\",\"choices\":[{\"delta\":{\"content\":\"before\"}}]}\n\n"
                .to_vec(),
        ],
        b"data: {\"id\":\"broken\"".to_vec(),
        128,
    )?;
    let api = TestApi::start(upstream.port)?;

    let response = post_chat(api.port, &api.api_key, &stream_request_body())?;
    let data = sse_data_lines(&response.body);
    let last = data
        .last()
        .ok_or("expected at least one SSE data line from stream error")?;
    let error: Value = serde_json::from_str(last)?;

    assert_eq!(
        error
            .pointer("/error/code")
            .and_then(Value::as_str)
            .ok_or("missing error code")?,
        "upstream_stream_error"
    );

    Ok(())
}

#[test]
fn phase10c_non_streaming_path_unchanged() -> Result<(), Box<dyn std::error::Error>> {
    let upstream = MockServer::start();
    let upstream_body = json!({
        "id": "chatcmpl-test",
        "choices": [{"message": {"content": "plain json"}}]
    });
    let mock = upstream.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200)
            .header("content-type", "application/json")
            .body(upstream_body.to_string());
    });
    let api = TestApi::start(upstream.port())?;

    let response = post_chat(
        api.port,
        &api.api_key,
        &json!({
            "model": "local-coder",
            "stream": false,
            "messages": [{"role": "user", "content": "hello"}]
        })
        .to_string(),
    )?;

    mock.assert();
    assert!(response
        .headers
        .to_ascii_lowercase()
        .contains("content-type: application/json"));
    assert_eq!(
        serde_json::from_str::<Value>(&response.body)?,
        upstream_body
    );

    Ok(())
}

struct TestApi {
    port: u16,
    api_key: String,
    _join: JoinHandle<()>,
    _temp: TempDir,
}

impl TestApi {
    fn start(upstream_port: u16) -> Result<Self, Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let models_yaml_path = temp.path().join("models.yaml");
        fs::write(&models_yaml_path, model_registry_yaml(upstream_port))?;
        let db_path = temp.path().join("skycode.db");
        let conn = Connection::open(&db_path)?;
        let api_key = "phase10c-api-key".to_string();
        let state = AppState {
            api_key: Arc::new(api_key.clone()),
            models_yaml_path,
            db_path,
            project_root: temp.path().to_path_buf(),
            http_client: reqwest::Client::new(),
            conn: Arc::new(Mutex::new(conn)),
        };

        let port = find_free_port()?;
        let join = thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(_) => return,
            };
            runtime.block_on(async move {
                let _ = run(state, "127.0.0.1", port).await;
            });
        });
        wait_for_port(port)?;

        Ok(Self {
            port,
            api_key,
            _join: join,
            _temp: temp,
        })
    }
}

struct ScriptedUpstream {
    port: u16,
    join: Option<JoinHandle<()>>,
}

impl ScriptedUpstream {
    fn complete(chunks: Vec<Vec<u8>>) -> Result<Self, Box<dyn std::error::Error>> {
        Self::spawn(UpstreamScript::Complete { chunks })
    }

    fn broken_after(
        chunks: Vec<Vec<u8>>,
        partial: Vec<u8>,
        declared_size: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::spawn(UpstreamScript::Broken {
            chunks,
            partial,
            declared_size,
        })
    }

    fn spawn(script: UpstreamScript) -> Result<Self, Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let join = thread::spawn(move || {
            let _ = serve_scripted_upstream(listener, script);
        });
        Ok(Self {
            port,
            join: Some(join),
        })
    }
}

impl Drop for ScriptedUpstream {
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            let _ = join.join();
        }
    }
}

enum UpstreamScript {
    Complete {
        chunks: Vec<Vec<u8>>,
    },
    Broken {
        chunks: Vec<Vec<u8>>,
        partial: Vec<u8>,
        declared_size: usize,
    },
}

struct HttpResponse {
    headers: String,
    body: String,
}

fn serve_scripted_upstream(
    listener: TcpListener,
    script: UpstreamScript,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut stream, _) = listener.accept()?;
    let _ = read_http_request(&mut stream);
    stream.write_all(
        b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n",
    )?;

    match script {
        UpstreamScript::Complete { chunks } => {
            for chunk in chunks {
                write_http_chunk(&mut stream, &chunk)?;
            }
            stream.write_all(b"0\r\n\r\n")?;
        }
        UpstreamScript::Broken {
            chunks,
            partial,
            declared_size,
        } => {
            for chunk in chunks {
                write_http_chunk(&mut stream, &chunk)?;
            }
            stream.write_all(format!("{declared_size:x}\r\n").as_bytes())?;
            stream.write_all(&partial)?;
            stream.flush()?;
        }
    }

    Ok(())
}

fn write_http_chunk(stream: &mut TcpStream, chunk: &[u8]) -> Result<(), std::io::Error> {
    stream.write_all(format!("{:x}\r\n", chunk.len()).as_bytes())?;
    stream.write_all(chunk)?;
    stream.write_all(b"\r\n")?;
    stream.flush()?;
    thread::sleep(Duration::from_millis(15));
    Ok(())
}

fn post_chat(
    port: u16,
    api_key: &str,
    body: &str,
) -> Result<HttpResponse, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let request = format!(
        "POST /v1/chat/completions HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Authorization: Bearer {api_key}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        body.len()
    );
    stream.write_all(request.as_bytes())?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    decode_http_response(&raw)
}

fn decode_http_response(raw: &[u8]) -> Result<HttpResponse, Box<dyn std::error::Error>> {
    let header_end = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| {
            format!(
                "HTTP response missing header terminator; raw={}",
                String::from_utf8_lossy(raw)
            )
        })?;
    let headers = String::from_utf8_lossy(&raw[..header_end]).to_string();
    let body = &raw[header_end + 4..];
    let decoded = if headers
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        decode_chunked(body)?
    } else {
        body.to_vec()
    };

    Ok(HttpResponse {
        headers,
        body: String::from_utf8_lossy(&decoded).to_string(),
    })
}

fn decode_chunked(mut body: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut decoded = Vec::new();

    loop {
        let line_end = body
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or("chunk size line missing terminator")?;
        let size_text = String::from_utf8_lossy(&body[..line_end]);
        let size_hex = size_text
            .split(';')
            .next()
            .ok_or("chunk size line missing size")?
            .trim();
        let size = usize::from_str_radix(size_hex, 16)?;
        body = &body[line_end + 2..];

        if size == 0 {
            break;
        }
        if body.len() < size + 2 {
            return Err("chunk body shorter than declared size".into());
        }

        decoded.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }

    Ok(decoded)
}

fn read_http_request(stream: &mut TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut buf = [0u8; 512];
    let mut request = Vec::new();
    loop {
        let read = stream.read(&mut buf)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buf[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    Ok(())
}

fn wait_for_port(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err(format!("server on port {port} did not become reachable").into())
}

fn find_free_port() -> Result<u16, Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

fn model_registry_yaml(port: u16) -> String {
    format!(
        "models:\n  - name: local-coder\n    runtime: local_gguf\n    path: /dev/null\n    ctx_size: 4096\n    gpu_layers: 0\n    strengths: [code_edit]\n    enabled: true\n    threads: 1\n    n_cpu_moe: ~\n    no_mmap: false\n    mlock: false\n    port: {port}\n"
    )
}

fn stream_request_body() -> String {
    json!({
        "model": "local-coder",
        "stream": true,
        "messages": [{"role": "user", "content": "hello"}]
    })
    .to_string()
}

fn sse_data_lines(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|data| data.trim_start().to_string())
        .collect()
}

fn content_from_data(data: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value: Value = serde_json::from_str(data)?;
    Ok(value
        .pointer("/choices/0/delta/content")
        .and_then(Value::as_str)
        .ok_or("missing streamed content")?
        .to_string())
}
