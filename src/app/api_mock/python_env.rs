use std::path::PathBuf;

pub fn api_mock_python_dir() -> PathBuf {
    #[cfg(test)]
    {
        return std::env::temp_dir().join("rriter_api_mock_python_tests");
    }
    #[cfg(not(test))]
    {
        crate::platform::data_dir().join("python-v1")
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
    crate::platform::atomic_write(&path, WORKER_SCRIPT.as_bytes())?;
    Ok(path)
}

const WORKER_SCRIPT: &str = r#"
import base64
import json
import sys
import traceback
from dataclasses import dataclass
from typing import Any, Annotated, Literal

def json_response(data, status=200, headers=None):
    return {"status": status, "headers": headers or {}, "json": data}

def text_response(text, status=200, headers=None):
    return {"status": status, "headers": headers or {}, "text": str(text)}

def error_response(message, status=500):
    return {"status": status, "headers": {}, "text": str(message)}

class AttrDict(dict):
    def __init__(self, values=None, **kwargs):
        merged = {}
        if values:
            merged.update(values)
        merged.update(kwargs)
        super().__init__(merged)
        self.__dict__ = self

@dataclass(init=False)
class BaseModel:
    def __init__(self, **values):
        for name, value in values.items():
            setattr(self, name, value)

class UploadFile(BaseModel):
    filename = ""
    content_type = None
    content = b""
    size = 0

class MaxLen:
    def __init__(self, value): pass

class MinLen:
    def __init__(self, value): pass

class Pattern:
    def __init__(self, value): pass

class Ge:
    def __init__(self, value): pass

class Gt:
    def __init__(self, value): pass

class Le:
    def __init__(self, value): pass

class Lt:
    def __init__(self, value): pass

class MinItems:
    def __init__(self, value): pass

class MaxItems:
    def __init__(self, value): pass

def _contract_class_names(source):
    names = []
    for line in source.splitlines():
        stripped = line.strip()
        if not stripped.startswith("class "):
            continue
        name = stripped[6:].split("(", 1)[0].split(":", 1)[0].strip()
        if name:
            names.append(name)
    return names

def _hidden_contract_line(line):
    stripped = line.lstrip()
    indent = line[:len(line) - len(stripped)]
    if not stripped.startswith("class Response"):
        return line
    colon = stripped.rfind(":")
    if colon < 0 or "BaseModel" in stripped[:colon]:
        return line
    header = stripped[len("class Response"):colon].strip()
    tail = stripped[colon:]
    if not header:
        return indent + "class Response(BaseModel)" + tail
    if header.startswith("(") and header.endswith(")"):
        args = header[1:-1].strip()
        if not args:
            return indent + "class Response(BaseModel)" + tail
        return indent + "class Response(BaseModel, " + args + ")" + tail
    return line

def _hidden_contract_source(source):
    return "\n".join(_hidden_contract_line(line) for line in source.splitlines())

def _model_init(self, **values):
    for name, value in values.items():
        setattr(self, name, value)

def _patch_model_classes(ns, names):
    for name in names:
        cls = ns.get(name)
        if isinstance(cls, type) and cls is not AttrDict:
            cls.__init__ = _model_init

def _to_json_value(value):
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, (bytes, bytearray)):
        return base64.b64encode(bytes(value)).decode("ascii")
    if isinstance(value, dict):
        return {str(key): _to_json_value(item) for key, item in value.items()}
    if isinstance(value, (list, tuple, set)):
        return [_to_json_value(item) for item in value]
    attrs = getattr(value, "__dict__", None)
    if attrs is not None:
        out = {}
        annotations = getattr(value.__class__, "__annotations__", {}) or {}
        for key in annotations:
            if not str(key).startswith("_") and hasattr(value, key):
                out[str(key)] = _to_json_value(getattr(value, key))
        for key, item in attrs.items():
            if not str(key).startswith("_"):
                out[str(key)] = _to_json_value(item)
        return out
    return value

def _response_envelope(value):
    return isinstance(value, dict) and any(
        key in value for key in ("status", "headers", "json", "text")
    )

def _normalize_result(result):
    if result is None:
        return json_response({})
    if _response_envelope(result):
        result.setdefault("status", 200)
        result.setdefault("headers", {})
        return result
    return json_response(_to_json_value(result))

def _upload_file_from_dict(value):
    raw = value.get("content_base64") or ""
    try:
        content = base64.b64decode(str(raw).encode("ascii"), validate=False)
    except Exception:
        content = b""
    size = value.get("size")
    if not isinstance(size, int):
        size = len(content)
    return UploadFile(
        filename=str(value.get("filename") or ""),
        content_type=value.get("content_type"),
        content=content,
        size=size,
    )

def _coerce_input_value(value):
    if isinstance(value, dict):
        if value.get("__rriter_type") == "file":
            return _upload_file_from_dict(value)
        return AttrDict({str(key): _coerce_input_value(item) for key, item in value.items()})
    if isinstance(value, list):
        return [_coerce_input_value(item) for item in value]
    return value

def _run(msg):
    prelude = msg.get("prelude") or ""
    body = msg.get("script_body")
    if body is None and isinstance(msg.get("body"), str):
        body = msg.get("body")
    body = body or "return Response(ok=True)"
    contract_source = msg.get("contract_source") or ""
    params = msg.get("params") or {}
    plan = msg.get("arg_plan") or {}
    ns = {
        "json_response": json_response,
        "text_response": text_response,
        "error_response": error_response,
        "AttrDict": AttrDict,
        "BaseModel": BaseModel,
        "UploadFile": UploadFile,
        "Response": AttrDict,
        "Any": Any,
        "Annotated": Annotated,
        "Literal": Literal,
        "MaxLen": MaxLen,
        "MinLen": MinLen,
        "Pattern": Pattern,
        "Ge": Ge,
        "Gt": Gt,
        "Le": Le,
        "Lt": Lt,
        "MinItems": MinItems,
        "MaxItems": MaxItems,
    }
    model_names = _contract_class_names(contract_source)
    if contract_source:
        exec("from __future__ import annotations\n" + _hidden_contract_source(contract_source), ns, ns)
    if "Response" not in model_names:
        model_names.append("Response")
    _patch_model_classes(ns, model_names)
    if prelude:
        exec(prelude, ns, ns)
    indented = "\n".join("    " + line for line in body.splitlines())
    if not indented.strip():
        indented = "    return Response(ok=True)"
    param_names = []
    for item in plan.get("path_args") or []:
        name = str(item.get("name") or "")
        clean = str(item.get("python_name") or "")
        if name and clean:
            param_names.append((name, clean))
    args = ["req"] + [clean for _, clean in param_names]
    if plan.get("query"):
        args.append("query")
    if plan.get("body"):
        args.append("body")
    if plan.get("fields"):
        args.append("fields")
    src = "def handler(" + ", ".join(args) + "):\n" + indented + "\n"
    exec(src, ns, ns)
    call_args = [AttrDict(msg.get("req") or {})]
    call_args.extend(params.get(raw) for raw, _ in param_names)
    if plan.get("query"):
        call_args.append(AttrDict(msg.get("query") or {}))
    if plan.get("body"):
        call_args.append(_coerce_input_value(msg.get("body")))
    if plan.get("fields"):
        call_args.append(_coerce_input_value(msg.get("fields") or {}))
    result = ns["handler"](
        *call_args
    )
    return _normalize_result(result)

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
        let worker = std::fs::read_to_string(&path).expect("worker");
        assert!(worker.contains("json_response"));
        assert!(worker.contains("contract_source"));
        assert!(worker.contains("script_body"));
        assert!(worker.contains("Response"));
        assert!(worker.contains("UploadFile"));
        assert!(worker.contains("base64.b64decode"));
        let _ = std::fs::remove_dir_all(api_mock_python_dir());
    }
}
