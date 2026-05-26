use super::python_env::write_api_mock_worker;
use super::types::ApiMockPythonScript;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

static WORKER: LazyLock<Mutex<Option<PythonWorker>>> = LazyLock::new(|| Mutex::new(None));

pub struct PythonMockRequest {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub params: BTreeMap<String, String>,
    pub query: BTreeMap<String, String>,
    pub body: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PythonMockResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
    pub content_type: &'static str,
}

struct PythonWorker {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<String>,
    next_id: u64,
    uv_path: PathBuf,
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
    uv_path: Option<&Path>,
    script: &ApiMockPythonScript,
    request: PythonMockRequest,
) -> Result<PythonMockResponse, String> {
    let uv_path = uv_path.ok_or_else(|| "uv path is not configured".to_string())?;
    let mut guard = WORKER
        .lock()
        .map_err(|_| "Python worker lock failed".to_string())?;
    let needs_start = guard
        .as_ref()
        .is_none_or(|worker| worker.uv_path.as_path() != uv_path);
    if needs_start {
        *guard = Some(start_worker(uv_path)?);
    }
    let worker = guard
        .as_mut()
        .ok_or_else(|| "Python worker missing".to_string())?;
    match worker.call(script, request) {
        Ok(response) => Ok(response),
        Err(err) => {
            let _ = worker.child.kill();
            *guard = None;
            Err(err)
        }
    }
}

fn start_worker(uv_path: &Path) -> Result<PythonWorker, String> {
    let worker_path = write_api_mock_worker().map_err(|err| err.to_string())?;
    let mut child = Command::new(uv_path)
        .arg("run")
        .arg("--no-project")
        .arg("--python")
        .arg("3.13")
        .arg(worker_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| err.to_string())?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Python worker stdin unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Python worker stdout unavailable".to_string())?;
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("rriter-api-mock-python-out".to_string())
        .spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        })
        .map_err(|err| err.to_string())?;
    Ok(PythonWorker {
        child,
        stdin,
        rx,
        next_id: 1,
        uv_path: uv_path.to_path_buf(),
    })
}

impl PythonWorker {
    fn call(
        &mut self,
        script: &ApiMockPythonScript,
        request: PythonMockRequest,
    ) -> Result<PythonMockResponse, String> {
        let id = self.next_id.max(1);
        self.next_id = self.next_id.saturating_add(1).max(1);
        let msg = json!({
            "id": id,
            "prelude": script.prelude,
            "body": script.body,
            "req": {
                "method": request.method,
                "path": request.path,
                "headers": request.headers,
            },
            "params": request.params,
            "query": request.query,
            "body": request.body,
            "fields": {},
        });
        serde_json::to_writer(&mut self.stdin, &msg).map_err(|err| err.to_string())?;
        self.stdin.write_all(b"\n").map_err(|err| err.to_string())?;
        self.stdin.flush().map_err(|err| err.to_string())?;
        let timeout = Duration::from_millis(script.timeout_ms.clamp(50, 30_000));
        let line = self
            .rx
            .recv_timeout(timeout)
            .map_err(|_| "Python mock timeout".to_string())?;
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
}
