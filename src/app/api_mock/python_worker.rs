use super::contract::{api_mock_contract_state_text, api_mock_worker_arg_plan};
use super::python_bootstrap::python_command;
use super::python_env::write_api_mock_worker;
use super::types::{ApiMockPythonScript, ApiPythonRuntimeConfig, ApiPythonRuntimeMode};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

const PYTHON_WORKER_SHUTDOWN_GRACE: Duration = Duration::from_millis(300);
const PYTHON_STDERR_LIMIT: usize = 64 * 1024;

static WORKER: LazyLock<Mutex<Option<PythonWorker>>> = LazyLock::new(|| Mutex::new(None));

pub struct PythonMockRequest {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub params: BTreeMap<String, Value>,
    pub query: BTreeMap<String, Value>,
    pub body: Value,
    pub fields: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PythonMockResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
    pub content_type: &'static str,
}

struct PythonWorker {
    child: crate::platform::ManagedChild,
    stdin: ChildStdin,
    rx: Receiver<String>,
    stderr: Arc<Mutex<String>>,
    next_id: u64,
    runtime: ApiPythonRuntimeConfig,
}

#[derive(Deserialize)]
struct WorkerOutput {
    id: Option<u64>,
    status: Option<u16>,
    headers: Option<BTreeMap<String, String>>,
    json: Option<Value>,
    text: Option<String>,
}

pub fn call_python_route(
    runtime: &ApiPythonRuntimeConfig,
    script: &ApiMockPythonScript,
    request: PythonMockRequest,
) -> Result<PythonMockResponse, String> {
    let mut guard = WORKER
        .lock()
        .map_err(|_| "Python worker lock failed".to_string())?;
    let needs_start = guard
        .as_ref()
        .is_none_or(|worker| worker.runtime != *runtime);
    if needs_start {
        *guard = Some(start_worker(runtime)?);
    }
    let worker = guard
        .as_mut()
        .ok_or_else(|| "Python worker missing".to_string())?;
    match worker.call(script, request) {
        Ok(response) => Ok(response),
        Err(error) => {
            let detail = worker.stderr_text();
            *guard = None;
            if detail.is_empty() {
                Err(error)
            } else {
                Err(format!("{error}\nPython stderr:\n{detail}"))
            }
        }
    }
}

pub fn stop_python_worker() {
    if let Ok(mut guard) = WORKER.lock() {
        if let Some(mut worker) = guard.take() {
            let _ = worker.child.terminate(PYTHON_WORKER_SHUTDOWN_GRACE);
        }
    }
}

fn start_worker(runtime: &ApiPythonRuntimeConfig) -> Result<PythonWorker, String> {
    let worker_path = write_api_mock_worker().map_err(|err| err.to_string())?;
    let mut command = python_worker_command(runtime, worker_path)?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = crate::platform::ManagedChild::spawn(&mut command)
        .map_err(|err| format!("failed to start Python worker: {err}"))?;
    let stdin = child
        .take_stdin()
        .ok_or_else(|| "Python worker stdin unavailable".to_string())?;
    let stdout = child
        .take_stdout()
        .ok_or_else(|| "Python worker stdout unavailable".to_string())?;
    let stderr = child
        .take_stderr()
        .ok_or_else(|| "Python worker stderr unavailable".to_string())?;

    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("rriter-api-mock-python-out".to_string())
        .spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        })
        .map_err(|err| err.to_string())?;

    let stderr_text = Arc::new(Mutex::new(String::new()));
    let stderr_target = stderr_text.clone();
    std::thread::Builder::new()
        .name("rriter-api-mock-python-err".to_string())
        .spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let Ok(mut stored) = stderr_target.lock() else {
                    break;
                };
                if stored.len() >= PYTHON_STDERR_LIMIT {
                    continue;
                }
                if !stored.is_empty() {
                    stored.push('\n');
                }
                let remaining = PYTHON_STDERR_LIMIT.saturating_sub(stored.len());
                let mut end = line.len().min(remaining);
                while end > 0 && !line.is_char_boundary(end) {
                    end -= 1;
                }
                stored.push_str(&line[..end]);
            }
        })
        .map_err(|err| err.to_string())?;

    Ok(PythonWorker {
        child,
        stdin,
        rx,
        stderr: stderr_text,
        next_id: 1,
        runtime: runtime.clone(),
    })
}

fn python_worker_command(
    runtime: &ApiPythonRuntimeConfig,
    worker_path: PathBuf,
) -> Result<Command, String> {
    match runtime.mode {
        ApiPythonRuntimeMode::UvManaged => {
            let uv_path = runtime
                .uv_path
                .as_ref()
                .ok_or_else(|| "uv path is not configured".to_string())?;
            let version = if runtime.python_version.trim().is_empty() {
                "3.13"
            } else {
                runtime.python_version.trim()
            };
            let mut command = Command::new(uv_path);
            command
                .arg("run")
                .arg("--no-project")
                .arg("--python")
                .arg(version)
                .arg(worker_path)
                .env("UV_PYTHON_DOWNLOADS", "never");
            Ok(command)
        }
        ApiPythonRuntimeMode::CustomPython => {
            let python_path = runtime
                .custom_python_path
                .as_ref()
                .ok_or_else(|| "Python path is not configured".to_string())?;
            let mut command = python_command(python_path);
            command.arg(worker_path);
            Ok(command)
        }
    }
}

impl PythonWorker {
    fn call(
        &mut self,
        script: &ApiMockPythonScript,
        request: PythonMockRequest,
    ) -> Result<PythonMockResponse, String> {
        if self.child.try_wait().map_err(|err| err.to_string())?.is_some() {
            return Err("Python worker exited before handling the request".to_string());
        }

        let id = self.next_id.max(1);
        self.next_id = self.next_id.saturating_add(1).max(1);
        let msg = json!({
            "id": id,
            "prelude": script.prelude,
            "script_body": script.body,
            "contract_source": if script.contract_source.trim().is_empty() {
                api_mock_contract_state_text(&script.contract)
            } else {
                script.contract_source.clone()
            },
            "arg_plan": api_mock_worker_arg_plan(&script.contract),
            "req": {
                "method": request.method,
                "path": request.path,
                "headers": request.headers,
            },
            "params": request.params,
            "query": request.query,
            "body": request.body,
            "fields": request.fields,
        });
        serde_json::to_writer(&mut self.stdin, &msg).map_err(|err| err.to_string())?;
        self.stdin.write_all(b"\n").map_err(|err| err.to_string())?;
        self.stdin.flush().map_err(|err| err.to_string())?;
        let timeout = Duration::from_millis(script.timeout_ms.clamp(50, 30_000));
        let line = self
            .rx
            .recv_timeout(timeout)
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => "Python mock timeout".to_string(),
                mpsc::RecvTimeoutError::Disconnected => {
                    "Python worker output stream closed".to_string()
                }
            })?;
        let output: WorkerOutput = serde_json::from_str(&line).map_err(|err| err.to_string())?;
        if output.id != Some(id) {
            return Err("Python mock response id mismatch".to_string());
        }
        let status = output.status.unwrap_or(200);
        let headers = output.headers.unwrap_or_default();
        if let Some(value) = output.json {
            let body = serde_json::to_string(&value).map_err(|err| err.to_string())?;
            Ok(PythonMockResponse {
                status,
                headers,
                body,
                content_type: "application/json",
            })
        } else {
            Ok(PythonMockResponse {
                status,
                headers,
                body: output.text.unwrap_or_default(),
                content_type: "text/plain; charset=utf-8",
            })
        }
    }

    fn stderr_text(&self) -> String {
        self.stderr
            .lock()
            .map(|text| text.trim().to_string())
            .unwrap_or_default()
    }
}

impl Drop for PythonWorker {
    fn drop(&mut self) {
        let _ = self.child.terminate(PYTHON_WORKER_SHUTDOWN_GRACE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_command_uses_uv_managed_python_version() {
        let runtime = ApiPythonRuntimeConfig {
            mode: ApiPythonRuntimeMode::UvManaged,
            uv_path: Some(PathBuf::from(r"C:\Program Files\uv\uv.exe")),
            custom_python_path: None,
            python_version: "3.12".to_string(),
        };

        let command = python_worker_command(
            &runtime,
            PathBuf::from(r"C:\Users\Reyan\Mock Project\worker.py"),
        )
        .unwrap();
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        assert_eq!(command.get_program(), r"C:\Program Files\uv\uv.exe");
        assert_eq!(
            args,
            vec![
                "run",
                "--no-project",
                "--python",
                "3.12",
                r"C:\Users\Reyan\Mock Project\worker.py",
            ]
        );
    }

    #[test]
    fn worker_command_uses_custom_python_path() {
        let runtime = ApiPythonRuntimeConfig {
            mode: ApiPythonRuntimeMode::CustomPython,
            uv_path: None,
            custom_python_path: Some(PathBuf::from("/opt/python/bin/python")),
            python_version: "3.13".to_string(),
        };

        let command = python_worker_command(&runtime, PathBuf::from("/tmp/worker.py")).unwrap();
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        assert_eq!(command.get_program(), "/opt/python/bin/python");
        assert_eq!(args, vec!["/tmp/worker.py"]);
    }

    #[test]
    fn worker_command_supports_windows_python_launcher() {
        let runtime = ApiPythonRuntimeConfig {
            mode: ApiPythonRuntimeMode::CustomPython,
            uv_path: None,
            custom_python_path: Some(PathBuf::from(r"C:\Windows\py.exe")),
            python_version: "3.13".to_string(),
        };

        let command = python_worker_command(
            &runtime,
            PathBuf::from(r"C:\Users\Re YaN\worker.py"),
        )
        .unwrap();
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(args, vec!["-3", r"C:\Users\Re YaN\worker.py"]);
    }
}
