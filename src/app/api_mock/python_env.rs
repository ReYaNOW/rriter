use std::path::PathBuf;

pub fn api_mock_python_dir() -> PathBuf {
    #[cfg(test)]
    {
        return std::env::temp_dir().join("rriter_api_mock_python_tests");
    }
    #[cfg(not(test))]
    {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .unwrap_or_default();
        base.join("rriter").join("python")
    }
}

pub fn api_mock_worker_path() -> PathBuf {
    api_mock_python_dir().join("worker.py")
}

pub fn write_api_mock_worker() -> std::io::Result<PathBuf> {
    let path = api_mock_worker_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, WORKER_SCRIPT.as_bytes())?;
    Ok(path)
}

const WORKER_SCRIPT: &str = r#"
import json
import sys
import traceback

def json_response(data, status=200, headers=None):
    return {"status": status, "headers": headers or {}, "json": data}

def text_response(text, status=200, headers=None):
    return {"status": status, "headers": headers or {}, "text": str(text)}

def error_response(message, status=500):
    return {"status": status, "headers": {}, "text": str(message)}

def _run(msg):
    prelude = msg.get("prelude") or ""
    body = msg.get("body") or "return json_response({})"
    params = msg.get("params") or {}
    ns = {
        "json_response": json_response,
        "text_response": text_response,
        "error_response": error_response,
    }
    if prelude:
        exec(prelude, ns, ns)
    indented = "\n".join("    " + line for line in body.splitlines())
    param_names = []
    for name in params:
        clean = "".join(ch if ch.isalnum() or ch == "_" else "_" for ch in str(name))
        if not clean or clean[0].isdigit():
            clean = "_" + clean
        if clean in {"req", "query", "body", "fields"}:
            clean = clean + "_param"
        param_names.append((name, clean))
    args = ["req"] + [clean for _, clean in param_names] + ["query", "body", "fields"]
    src = "def handler(" + ", ".join(args) + "):\n" + indented + "\n"
    exec(src, ns, ns)
    call_args = [msg.get("req") or {}]
    call_args.extend(params.get(raw) for raw, _ in param_names)
    call_args.extend([msg.get("query") or {}, msg.get("body"), msg.get("fields") or {}])
    result = ns["handler"](
        *call_args
    )
    if result is None:
        return json_response({})
    return result

for line in sys.stdin:
    try:
        msg = json.loads(line)
        out = _run(msg)
        out["id"] = msg.get("id")
    except Exception as exc:
        out = {
            "id": msg.get("id") if "msg" in locals() else None,
            "status": 500,
            "headers": {},
            "text": str(exc),
            "traceback": traceback.format_exc(),
        }
    sys.stdout.write(json.dumps(out, separators=(",", ":")) + "\n")
    sys.stdout.flush()
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_script_is_written_to_data_dir() {
        let _ = std::fs::remove_dir_all(api_mock_python_dir());
        let path = write_api_mock_worker().expect("write worker");
        assert!(path.ends_with("worker.py"));
        assert!(
            std::fs::read_to_string(&path)
                .expect("worker")
                .contains("json_response")
        );
        let _ = std::fs::remove_dir_all(api_mock_python_dir());
    }
}
