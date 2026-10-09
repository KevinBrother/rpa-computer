"""Shared deterministic case data; no IO or platform activation."""
import hashlib
import json

SCHEMA = "windows-crossapp-pack-v1"
CASE_IDS = ["crossapp-%02d" % n for n in range(1, 7)]
TOOLS = ["computer_" + n for n in ("describe", "open", "observe", "step", "get_step", "pause", "resume", "close")]
APPLICATIONS = ["Notepad", "Calculator", "Explorer", "offline browser", "owned native drag fixture", "owned native focus fixture"]


def text_payload(nonce):
    return "ASCII: Hello 123\r\nUnicode: 中文 Ω 😀 e\u0301\r\nNonce: " + nonce + "\r\nTrailing space: END \r\n"


def explorer_bytes(case, index):
    return ("Owned crossapp-03 file %02d\r\nrun=%s\r\ntrial=%s\r\nnonce=%s\r\n" % (index, case["run_id"], case["trial_id"], case["nonce"])).encode("utf-8")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def json_bytes(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def ownership_pending():
    return dict(status="needs_preflight", **{k: None for k in ("owned", "exe_path", "exe_sha256", "pid", "creation_time", "hwnd", "interactive_session_id", "visible_title_nonce")})
