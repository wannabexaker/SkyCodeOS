//! Phase 11 - scos serve auto-spawn lifecycle.
//!
//! These tests do not exercise the real llama-server. They exercise the
//! serve spawn decision and the API proxy path with a stub upstream.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use httpmock::{Method::POST, MockServer};
use rusqlite::Connection;
use serde_json::json;
use skycode_api::server::run_with_shutdown;
use skycode_api::state::AppState;
use skycode_cli::commands::serve::{prepare_llama_server_with_launcher, SpawnedLlama};
use tempfile::TempDir;
use tokio::sync::oneshot;

#[test]
fn phase11_serve_spawns_and_proxies_to_upstream() -> Result<(), Box<dyn std::error::Error>> {
    let upstream = MockServer::start();
    let mock = upstream.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200)
            .header("content-type", "application/json")
            .body(
                json!({
                    "id": "phase11-chat",
                    "choices": [{"message": {"content": "ok"}}]
                })
                .to_string(),
            );
    });

    let temp = TempDir::new()?;
    let repo = write_project(&temp, upstream.port())?;
    let spawned = prepare_llama_server_with_launcher(&repo, false, |launch| {
        assert_eq!(launch.port, upstream.port());
        Ok(((), 4242))
    })?;
    assert!(spawned.is_some(), "serve must spawn by default");

    let api = TestApi::start(&repo, upstream.port())?;
    let response = post_chat(api.port, &api.api_key, &chat_body())?;

    mock.assert();
    assert!(response.headers.contains("200 OK"), "{}", response.headers);
    assert!(response.body.contains("\"ok\""), "{}", response.body);
    Ok(())
}

#[test]
fn phase11_serve_no_spawn_flag_skips_child() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let upstream_port = find_free_port()?;
    let repo = write_project(&temp, upstream_port)?;
    let spawned: Option<SpawnedLlama<()>> =
        prepare_llama_server_with_launcher(&repo, true, |_launch| {
            Err("launcher should not be called in --no-spawn mode".into())
        })?;
    assert!(spawned.is_none(), "--no-spawn must not launch a child");

    let api = TestApi::start(&repo, upstream_port)?;
    let response = post_chat(api.port, &api.api_key, &chat_body())?;

    assert!(
        response.headers.contains("502 Bad Gateway"),
        "{}",
        response.headers
    );
    assert!(response.body.contains("model backend unreachable"));
    Ok(())
}

struct TestApi {
    port: u16,
    api_key: String,
    shutdown: Option<oneshot::Sender<()>>,
    join: Option<JoinHandle<()>>,
    _temp_guard: TempDir,
}

impl TestApi {
    fn start(repo: &Path, upstream_port: u16) -> Result<Self, Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let db_path = temp.path().join("skycode.db");
        let conn = Connection::open(&db_path)?;
        let api_key = "phase11-api-key".to_string();
        let state = AppState {
            api_key: Arc::new(api_key.clone()),
            models_yaml_path: repo.join("agents").join("models.yaml"),
            db_path,
            project_root: repo.to_path_buf(),
            http_client: reqwest::Client::new(),
            conn: Arc::new(Mutex::new(conn)),
        };
        let port = find_free_port()?;
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let join = thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(_) => return,
            };
            runtime.block_on(async move {
                let _ = run_with_shutdown(state, "127.0.0.1", port, async move {
                    let _ = shutdown_rx.await;
                })
                .await;
            });
        });
        wait_for_port(port)?;
        let _ = upstream_port;

        Ok(Self {
            port,
            api_key,
            shutdown: Some(shutdown_tx),
            join: Some(join),
            _temp_guard: temp,
        })
    }
}

impl Drop for TestApi {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct HttpResponse {
    headers: String,
    body: String,
}

fn write_project(
    temp: &TempDir,
    upstream_port: u16,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let repo = temp.path().join("repo");
    fs::create_dir_all(repo.join("agents"))?;
    fs::write(
        repo.join("agents").join("models.yaml"),
        model_registry_yaml(upstream_port),
    )?;
    Ok(repo)
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
        .ok_or("HTTP response missing header terminator")?;
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
        let size_hex = String::from_utf8_lossy(&body[..line_end]);
        let size = usize::from_str_radix(size_hex.trim(), 16)?;
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
        "models:\n  - name: local-coder\n    runtime: local_gguf\n    executable: stub-llama-server\n    path: stub-model.gguf\n    ctx_size: 4096\n    gpu_layers: 0\n    strengths: [code_edit]\n    enabled: true\n    threads: 1\n    n_cpu_moe: ~\n    no_mmap: false\n    mlock: false\n    port: {port}\n"
    )
}

fn chat_body() -> String {
    json!({
        "model": "local-coder",
        "messages": [{"role": "user", "content": "hello"}]
    })
    .to_string()
}
