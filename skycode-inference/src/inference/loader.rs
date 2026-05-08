use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, ExitStatus};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use skycode_tools::tools::process::spawn_piped_command;
use thiserror::Error;

use super::registry::{SplitMode, VramBudget};

const SERVER_HOST: &str = "127.0.0.1";
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(500);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(300);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone)]
pub struct ModelLaunchOptions {
    pub executable: Option<String>,
    pub model_path: PathBuf,
    pub ctx_size: usize,
    pub threads: usize,
    pub n_gpu_layers: usize,
    pub n_cpu_moe: Option<usize>,
    pub prompt: Option<String>,
    pub temp: f32,
    pub repeat_penalty: f32,
    pub no_mmap: bool,
    pub mlock: bool,
    pub kv_offload: bool,
    pub tensor_split: Vec<f64>,
    pub split_mode: SplitMode,
    pub vram_budget_mb: Option<VramBudget>,
    pub port: u16,
}

pub struct ModelHandle {
    pub process: Child,
    stderr_lines: Receiver<String>,
    stdout_lines: Receiver<String>,
    client: reqwest::blocking::Client,
    base_url: String,
    stopped: bool,
    pub mlock_verified: bool,
    pub mlock_warning: Option<String>,
}

#[derive(Debug, Error)]
pub enum InferenceError {
    #[error("model file not found: {0}")]
    ModelFileMissing(String),
    #[error("io error during model subprocess handling: {0}")]
    Io(#[from] std::io::Error),
    #[error("http error during model request: {0}")]
    Http(#[from] reqwest::Error),
    #[error("subprocess stdout unavailable")]
    MissingStdout,
    #[error("subprocess stderr unavailable")]
    MissingStderr,
    #[error("llama-server exited before becoming healthy: status={status}, stderr={stderr}")]
    ServerExited { status: String, stderr: String },
    #[error("llama-server health check timed out after {seconds}s")]
    ServerHealthTimeout { seconds: u64 },
    #[error("model output invalid: {0}")]
    ModelOutputInvalid(String),
    #[error("{0}")]
    UnsupportedOperation(&'static str),
}

pub type ModelLoadError = InferenceError;

pub fn launch_model(options: &ModelLaunchOptions) -> Result<ModelHandle, ModelLoadError> {
    launch_server(options)
}

pub fn launch_server(options: &ModelLaunchOptions) -> Result<ModelHandle, InferenceError> {
    if !options.model_path.exists() {
        return Err(InferenceError::ModelFileMissing(
            options.model_path.display().to_string(),
        ));
    }

    let executable = options
        .executable
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .unwrap_or("llama-server");

    let argv = build_llama_server_argv(options);
    let mut process = spawn_piped_command(executable, &argv)?;
    let stdout = match process.stdout.take() {
        Some(stdout) => stdout,
        None => {
            terminate_child(&mut process);
            return Err(InferenceError::MissingStdout);
        }
    };
    let stderr = match process.stderr.take() {
        Some(stderr) => stderr,
        None => {
            terminate_child(&mut process);
            return Err(InferenceError::MissingStderr);
        }
    };
    let stdout_lines = spawn_line_reader(stdout);
    let stderr_lines = spawn_line_reader(stderr);
    let (mlock_verified, mlock_warning) = verify_mlock_status(&stderr_lines, options.mlock);
    let client = match reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            terminate_child(&mut process);
            return Err(InferenceError::Http(err));
        }
    };
    let base_url = format!("http://{}:{}", SERVER_HOST, options.port);

    if let Err(err) = poll_health(&client, &base_url, &mut process, &stderr_lines) {
        terminate_child(&mut process);
        return Err(err);
    }

    Ok(ModelHandle {
        process,
        stderr_lines,
        stdout_lines,
        client,
        base_url,
        stopped: false,
        mlock_verified,
        mlock_warning,
    })
}

pub fn build_llama_server_argv(options: &ModelLaunchOptions) -> Vec<String> {
    let mut argv = vec![
        "--model".to_string(),
        options.model_path.to_string_lossy().into_owned(),
        "--ctx-size".to_string(),
        options.ctx_size.to_string(),
        "--threads".to_string(),
        options.threads.to_string(),
        "--n-gpu-layers".to_string(),
        effective_gpu_layers(options).to_string(),
        "--port".to_string(),
        options.port.to_string(),
        "--host".to_string(),
        SERVER_HOST.to_string(),
    ];

    if let Some(n_cpu_moe) = options.n_cpu_moe {
        argv.push("--n-cpu-moe".to_string());
        argv.push(n_cpu_moe.to_string());
    }

    if options.no_mmap {
        argv.push("--no-mmap".to_string());
    }

    if options.mlock {
        argv.push("--mlock".to_string());
    }

    if !options.kv_offload {
        argv.push("--no-kv-offload".to_string());
    }

    if !options.tensor_split.is_empty() {
        argv.push("--tensor-split".to_string());
        argv.push(join_tensor_split(&options.tensor_split));
    }

    argv.push("--split-mode".to_string());
    argv.push(options.split_mode.as_flag().to_string());

    argv
}

fn effective_gpu_layers(options: &ModelLaunchOptions) -> usize {
    match options.vram_budget_mb {
        Some(VramBudget::Mb(0)) => 0,
        Some(VramBudget::Mb(_)) | Some(VramBudget::Auto(_)) | None => options.n_gpu_layers,
    }
}

fn join_tensor_split(tensor_split: &[f64]) -> String {
    tensor_split
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

pub fn call_model(prompt: &str, port: u16) -> Result<String, InferenceError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()?;
    let base_url = format!("http://{}:{}", SERVER_HOST, port);
    call_model_at(&client, &base_url, prompt)
}

fn spawn_line_reader<R>(stream: R) -> Receiver<String>
where
    R: Read + Send + 'static,
{
    let (tx, rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let _ = tx.send(line.trim().to_string());
                }
                Err(_) => break,
            }
        }
    });
    rx
}

fn poll_health(
    client: &reqwest::blocking::Client,
    base_url: &str,
    process: &mut Child,
    stderr_rx: &Receiver<String>,
) -> Result<(), InferenceError> {
    let health_url = format!("{}/health", base_url);
    let deadline = Instant::now() + HEALTH_TIMEOUT;

    while Instant::now() < deadline {
        if let Some(status) = process.try_wait()? {
            return Err(server_exited_error(status, stderr_rx));
        }

        match client.get(&health_url).send() {
            Ok(response) if response.status().is_success() => {
                if let Some(status) = process.try_wait()? {
                    return Err(server_exited_error(status, stderr_rx));
                }
                return Ok(());
            }
            Ok(_) | Err(_) => {
                thread::sleep(HEALTH_POLL_INTERVAL);
            }
        }
    }

    Err(InferenceError::ServerHealthTimeout {
        seconds: HEALTH_TIMEOUT.as_secs(),
    })
}

fn server_exited_error(status: ExitStatus, stderr_rx: &Receiver<String>) -> InferenceError {
    InferenceError::ServerExited {
        status: format_exit_status(status),
        stderr: drain_receiver(stderr_rx).join("\n"),
    }
}

fn format_exit_status(status: ExitStatus) -> String {
    match status.code() {
        Some(code) => code.to_string(),
        None => "terminated".to_string(),
    }
}

fn verify_mlock_status(stderr_rx: &Receiver<String>, requested: bool) -> (bool, Option<String>) {
    if !requested {
        return (false, None);
    }

    let mut mlock_warning: Option<String> = None;
    let deadline = Instant::now() + Duration::from_millis(1500);

    while Instant::now() < deadline {
        match stderr_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                if is_mlock_warning_line(&line) {
                    mlock_warning = Some(line);
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    if mlock_warning.is_none() {
        return (true, None);
    }

    (false, mlock_warning)
}

pub fn is_mlock_warning_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.contains("mlock")
        && (lower.contains("warning")
            || lower.contains("failed")
            || lower.contains("cannot")
            || lower.contains("disabled")
            || lower.contains("not supported"))
}

fn call_model_at(
    client: &reqwest::blocking::Client,
    base_url: &str,
    prompt: &str,
) -> Result<String, InferenceError> {
    let body = ChatCompletionRequest {
        model: "local",
        messages: vec![
            ChatMessageRequest {
                role: "system",
                content: "You are coder-primary. You MUST respond with a single valid JSON object. No markdown, no prose, no code fences. Only raw JSON.",
            },
            ChatMessageRequest {
                role: "user",
                content: prompt,
            },
        ],
        temperature: 0.1,
        max_tokens: 1024,
        response_format: ResponseFormat {
            format_type: "json_object",
        },
    };

    let response = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&body)
        .send()?
        .error_for_status()?;
    let response_text = response.text()?;
    let decoded = serde_json::from_str::<ChatCompletionResponse>(&response_text).map_err(|_| {
        InferenceError::ModelOutputInvalid("model server returned invalid chat JSON".to_string())
    })?;

    let choice = decoded.choices.into_iter().next().ok_or_else(|| {
        InferenceError::ModelOutputInvalid("model response choices were empty".to_string())
    })?;

    choice.message.content.ok_or_else(|| {
        InferenceError::ModelOutputInvalid("model response message content was missing".to_string())
    })
}

fn drain_receiver(rx: &Receiver<String>) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(line) = rx.try_recv() {
        out.push(line);
    }
    out
}

fn terminate_child(process: &mut Child) {
    if let Ok(None) = process.try_wait() {
        let _ = process.kill();
    }
    let _ = process.wait();
}

impl ModelHandle {
    pub fn call_model(&self, prompt: &str) -> Result<String, InferenceError> {
        call_model_at(&self.client, &self.base_url, prompt)
    }

    /// llama-server uses HTTP chat completions; prompt writes are unsupported.
    pub fn send_prompt(&mut self, _prompt: &str) -> Result<(), ModelLoadError> {
        Err(InferenceError::UnsupportedOperation(
            "llama-server prompts must be sent with call_model",
        ))
    }

    /// Return one captured stdout log line if one is immediately available.
    pub fn read_stdout_line(&mut self) -> Result<Option<String>, ModelLoadError> {
        match self.stdout_lines.try_recv() {
            Ok(line) => Ok(Some(line)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Ok(None),
        }
    }

    /// Pull any available stderr lines collected by the background reader.
    pub fn drain_stderr_lines(&self) -> Vec<String> {
        drain_receiver(&self.stderr_lines)
    }

    pub fn stop(&mut self) -> Result<(), ModelLoadError> {
        if self.stopped {
            return Ok(());
        }

        if self.process.try_wait()?.is_none() {
            match self.process.kill() {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::InvalidInput => {}
                Err(err) => return Err(InferenceError::Io(err)),
            }
        }

        let _ = self.process.wait();
        self.stopped = true;
        Ok(())
    }
}

impl Drop for ModelHandle {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessageRequest<'a>>,
    temperature: f32,
    max_tokens: usize,
    response_format: ResponseFormat,
}

#[derive(Debug, Serialize)]
struct ChatMessageRequest<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: &'static str,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
}
