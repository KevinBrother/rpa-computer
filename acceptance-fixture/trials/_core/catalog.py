"""Reviewed Windows task semantics, independent of test fixtures and their answers."""
from hashlib import sha256
from pathlib import Path

from .bounds import read_regular

ROOT = Path(__file__).resolve().parents[3]
ACTIONS = ("move", "click", "drag", "scroll", "text_input", "key_chord", "key_hold")
SUITE_ROUNDS = (("pointer", 4), ("multiclick", 1), ("drag", 1),
                ("scroll", 2), ("keyboard", 3), ("known-input", 1))
COVERAGE_LIMITATION = "Per-action quota only; not ten trials of every button/count/direction combination."
NATIVE_TEXT_POLICY = "windows-winforms-known09-crlf-v1"

# Pins are part of the reviewed v1 source contract. Never trust manifest-supplied paths.
# A changed task/catalog needs explicit re-review/versioning, even for a new campaign.
SOURCE_PINS = (
    ("acceptance-fixture/tasks/drag.md", "85a5f7968c35a332f6cbe34512d705e07c5551fdf0c91cf1e0b5551358e4091c"),
    ("acceptance-fixture/tasks/multiclick.md", "8c2dc2bf472bc2ea1382214c04747e4673c3d155f41542fe33f5f67d737eca88"),
    ("acceptance-fixture/tasks/scroll.md", "2e6e653acd54628beed89e204ef41b957478486b320ce183dae38e5622838393"),
    ("acceptance-fixture/tasks/windows-keyboard.md", "643ded8947dcb8ff668d89e93af2b0bd7e81af0c106c7334159b16f18bc2abc3"),
    ("acceptance-fixture/tasks/windows-pointer.md", "d257a9c0edf4a077810447b449f4362e2041b7ad4603f4fd64199cafefdd36fc"),
    ("acceptance-fixture/windows/BasicCases.cs", "9a40ca4bed926cf4da2cda2e3cdc4429cba6f1d04ec392e261e08c3f965fa6d1"),
    ("acceptance-fixture/windows/Cases.cs", "6bf1a8e1b33016f5dddde78a7350550b82914b8753dabd475bc217d40af44909"),
    ("acceptance-fixture/windows/GestureCases.cs", "456108a7ab12640ca1abbddc7a27fa2f8923af185e926d1ccb5208050bddbebe"),
    ("acceptance-fixture/windows/NativeTextPolicy.cs", "9bcc9c13293648ff096dc3ae8509987bcbf070b939c0b8671a94140a89ec3cfc"),
)

VARIANTS = {
    "pointer": ("center_move", "edge_move", "scaled_move", "left_click", "right_menu",
                "middle_click", "inactive_click", "small_target", "padding_reject", "bounds_reject"),
    "multiclick": ("flow=single;expect_count=1", "flow=multi;expect_count=2",
                  "flow=multi;expect_count=3", "flow=select_word", "flow=select_line",
                  "flow=two_targets", "flow=two_positions", "flow=right_between",
                  "flow=slow_two", "flow=invalid_count"),
    "drag": ("axis=h;dir=lr", "axis=h;dir=rl", "axis=v;dir=tb", "axis=v;dir=bt",
             "path=polyline", "span=short", "span=long;min_ms=750", "release=inner_edge",
             "flow=text_select", "path=curve"),
    "scroll": ("panel=A;axis=v;dir=down;ticks=3", "panel=A;axis=v;dir=up;ticks=3",
               "panel=A;axis=h;dir=right;ticks=3", "panel=A;axis=h;dir=left;ticks=3",
               "panel=A;axis=both;dir=down;dir2=right;ticks=3", "panel=B;axis=v;dir=down;ticks=3",
               "panel=A;axis=v;dir=down;ticks=1", "panel=A;axis=v;dir=down;ticks=20;flow=saturate",
               "flow=zero_delta", "flow=invalid_args"),
    "keyboard": ("plain_key", "enter_tab", "select_all", "shift_select", "alt_command",
                 "multi_modifier", "short_hold", "long_hold", "modifier_release", "invalid_key"),
    "known-input": ("ascii", "chinese_short", "chinese_long", "legacy_sample", "fullwidth",
                    "nbsp", "non_bmp", "nfd", "crlf", "tab"),
}
KNOWN_PAYLOADS = (
    "Order 6642 shipped via DHL Express",
    "键盘输入验收测试样例文本",
    "分布式原生输入验收涵盖中文长文本逐字精确比对三十四字样例内容全部完毕",
    "你好，世界 🌍 | Unicode 测试 ✅ | 本文档共 3 段（含本段）",
    "注意：括号（全角）与问号？感叹号！",
    "NBSP\u00a0分隔\u00a0样例",
    "🦄 独角兽 non-BMP 样例",
    "resume\u0301 和 cafe\u0301 组合音符样例",
    "第一行\r\n第二行\r\n第三行",
    "姓名\t部门\t工号",
)


def source_references():
    sources = []
    for relative, expected in SOURCE_PINS:
        try:
            digest = sha256(read_regular(ROOT / relative, 1024 * 1024)).hexdigest()
        except OSError as exc:
            raise ValueError(f"canonical source unavailable: {relative}") from exc
        if digest != expected:
            raise ValueError(f"canonical source drift; re-review required: {relative}")
        sources.append({"path": relative, "sha256": digest})
    return sources


def classification(suite, trial):
    if suite == "pointer":
        if trial == 9:
            return None, "blocked"
        return ("move" if trial <= 3 or trial == 10 else "click",
                "rejected" if trial == 10 else "eligible")
    if suite == "multiclick":
        return "click", "eligible" if trial <= 5 else "rejected" if trial == 10 else "coverage_only"
    if suite == "drag":
        return "drag", "eligible"
    if suite == "scroll":
        return "scroll", {8: "coverage_only", 9: "zero", 10: "rejected"}.get(trial, "eligible")
    if suite == "keyboard":
        action = "key_hold" if trial in (1, 2, 7, 8) else "key_chord"
        return action, {9: "coverage_only", 10: "rejected"}.get(trial, "eligible")
    return "text_input", "eligible"


def semantic_cases(suite):
    prefix = "known" if suite == "known-input" else suite
    for trial, variant in enumerate(VARIANTS[suite], 1):
        action, category = classification(suite, trial)
        yield {
            "suite": suite, "case_id": f"{prefix}-{trial:02d}", "trial": trial,
            "action": action, "classification": category, "eligible": category == "eligible",
            "variant": variant,
            "payload": KNOWN_PAYLOADS[trial - 1] if suite == "known-input" else None,
            "native_text_policy": NATIVE_TEXT_POLICY if suite == "known-input" and trial == 9 else None,
        }
