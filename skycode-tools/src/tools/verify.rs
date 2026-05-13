use std::io::{Error as IoError, ErrorKind, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

const STDIO_LIMIT: usize = 4096;
const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("no test command configured")]
    NoCommandConfigured,
    #[error("failed to spawn test command: {0}")]
    SpawnFailed(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyOutcome {
    pub exit_code: i32,
    pub stdout_truncated: String,
    pub stderr_truncated: String,
    pub elapsed_ms: u64,
    pub timed_out: bool,
}

pub fn run_verify(
    project_root: &Path,
    cmd: &str,
    timeout_secs: u64,
) -> Result<VerifyOutcome, VerifyError> {
    if cmd.trim().is_empty() {
        return Err(VerifyError::NoCommandConfigured);
    }

    let start = Instant::now();
    let mut command = shell_command(cmd);
    command
        .current_dir(project_root)
        .env("HOME", std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    strip_skycode_env(&mut command);

    let mut child = command.spawn()?;
    let stdout = child.stdout.take().ok_or_else(|| {
        VerifyError::SpawnFailed(IoError::new(
            ErrorKind::Other,
            "test command stdout pipe was unavailable",
        ))
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        VerifyError::SpawnFailed(IoError::new(
            ErrorKind::Other,
            "test command stderr pipe was unavailable",
        ))
    })?;
    let stdout_reader = thread::spawn(move || read_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_pipe(stderr));

    let timeout = Duration::from_secs(timeout_secs);
    let deadline = start + timeout;
    let mut timed_out = false;
    let exit_code;

    loop {
        if let Some(status) = child.try_wait()? {
            exit_code = status.code().unwrap_or(-1);
            break;
        }

        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            let _ = child.wait();
            exit_code = -1;
            break;
        }

        thread::sleep(POLL_INTERVAL);
    }

    // On timeout the immediate child is killed, but grandchildren spawned by shell
    // wrappers (for example, cmd.exe to ping.exe on Windows) may still hold the
    // stdio pipes open. Waiting indefinitely for reader threads would block for
    // the full duration of the grandchild. Skip them when timed out.
    let (stdout_bytes, stderr_bytes) = if timed_out {
        (Vec::new(), Vec::new())
    } else {
        let stdout_bytes = stdout_reader.join().map_err(|_| {
            VerifyError::SpawnFailed(IoError::new(ErrorKind::Other, "stdout reader panicked"))
        })??;
        let stderr_bytes = stderr_reader.join().map_err(|_| {
            VerifyError::SpawnFailed(IoError::new(ErrorKind::Other, "stderr reader panicked"))
        })??;
        (stdout_bytes, stderr_bytes)
    };
    let stdout_truncated = truncate_stdio(&stdout_bytes);
    let stderr_truncated = truncate_stdio(&stderr_bytes);
    let elapsed_ms = elapsed_millis(start.elapsed())?;

    Ok(VerifyOutcome {
        exit_code,
        stdout_truncated,
        stderr_truncated,
        elapsed_ms,
        timed_out,
    })
}

#[cfg(windows)]
fn shell_command(cmd: &str) -> Command {
    let mut command = Command::new("cmd");
    command.arg("/C").arg(cmd);
    command
}

#[cfg(not(windows))]
fn shell_command(cmd: &str) -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(cmd);
    command
}

fn strip_skycode_env(command: &mut Command) {
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("SKYCODE_") {
            command.env_remove(key);
        }
    }
}

fn read_pipe<R: Read>(mut pipe: R) -> Result<Vec<u8>, IoError> {
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn truncate_stdio(bytes: &[u8]) -> String {
    let end = bytes.len().min(STDIO_LIMIT);
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn elapsed_millis(duration: Duration) -> Result<u64, VerifyError> {
    u64::try_from(duration.as_millis()).map_err(|_| {
        VerifyError::SpawnFailed(IoError::new(
            ErrorKind::Other,
            "verify elapsed time overflowed u64",
        ))
    })
}
