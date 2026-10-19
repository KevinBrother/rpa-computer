# Focus boundary minimal GREEN candidate — frozen, syntax only

2026-10-16。仅修改 `desktop-feedback/linux/testing/live-ui/case.py` 的 `_arm_focus`。
未执行测试、SSH、GUI、Xvfb、安装、编译产品或修改环境；等待独立review与CC，本文不是通过声明。

## 已观测RED与最小实现

已读 `.agents/runs/linux-parity-20261015/cc-ui-focus-boundary-red-1/probe.log`：
新13四指定assertion FAIL / 九PASS / 零ERROR，`NEW13_RC=1`；旧27/13/Rust5通过。
outer shell最后echo返回0不代表新13通过；这些是RED snapshot结果，不转移为本次GREEN结果。

真实 `Case.start` 的setup move响应必须 `setup['focus']==self.o.target`，否则在Renderer构造前失败。
通过后保存 `len(self.o.events)`，记录 `focus_guard_armed`（cursor、target）再构造Renderer。
setup move已有fence/drain，无新增X请求。`ready=False`走同一边界。
inspect/image/ring前图/ring最终fence仍共用原 `_check_focus` 固定suffix；
没有清除历史、在Ready/运行期刷新cursor、修改rules或放宽瞬时失焦检查。
没有新增diagnostic sink、observer/产品/provider/Host改动。

## 检查与继承关系

- 改前RED完整46清单逐项匹配；改后仅case.py不同，其余45个文件逐字节相同。
- 新13与旧45所有断言、support、observer/rules/ipc保持不变。
- `testing/atom-bootstrap-green-1/source.sha256` 产品76逐项匹配。
- `compile(case_source, path, 'exec')` 内存syntax检查通过；无import/执行/pycache。
- 两个新archive逐项读取并核对SHA通过。
- 原RED清单、archive、handoff、syntax/audit日志均保留为历史；它们不是当前case.py验收记录。

## 冻结件（均位于本目录）

完整46使用RED相同路径集合，仅替换case.py哈希；小overlay仅1个case.py。
本交接说明与清单作为外部metadata，不计入46/1归档；不覆盖历史RED说明以避免改写记录。

| 文件 | SHA256 |
|---|---|
| `focus-green.sha256`（46条） | `ec06578840c372aa546ac9e9521b4eb99f10f7afa676b0082a92683c03cb2232` |
| `fixture-focus-green-1.tar.gz`（46文件） | `44cf29c743785e765b9dbe3b4fe60f3984ab418fdc130a2a0f3239e891ad164f` |
| `overlay.sha256`（1条） | `d22c779b63a6b5301f3e5c23cef2b42cb5b36505524c153f7fc97ecfc2314a65` |
| `fixture-focus-green-1-overlay.tar.gz`（1文件） | `8f73ba40fd404d4596b387d8b57bb3acb84ab82114a70a98a765b83f9ebed43d` |
| `../case.py` | `123478a04eab170189ec5abd5c6793100adc6b203e05dce31b0b629ede8f7521` |

## 独立CC命令（只交付，未执行）

在repository root使用既有approved依赖/vendor；unset DISPLAY/XAUTHORITY/WAYLAND_DISPLAY。
分别保留inner rc与case数，不能仅依赖末尾echo的outer rc。

```sh
L="$PWD/desktop-feedback/linux/testing/live-ui"
sha256sum -c "$L/focus_green/focus-green.sha256"
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$L:$L/focus_red" \
  python3 -m unittest discover -s "$L/focus_red" -p 'test_focus_boundary.py' -v
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$L" \
  python3 -m unittest discover -s "$L/tests" -v
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$L:$L/review_red" \
  python3 -m unittest discover -s "$L/review_red" -p 'test_*_red.py' -v
cargo-1.89 test --locked --offline --manifest-path "$L/observer/Cargo.toml" \
  --bin rpa-feedback-live-observer
```

目标计数新13＋旧27/13/Rust5，须独立实际验证。当前没有新测试/GUI结果。
review和CC通过后仍须main另行调度GUI，不能自动重跑。
历史GUI rawFAIL原样保留，不称为产品抢焦点；旧startup失败具体X请求仍未知。
capture exclusion仍unsupported，Host admission关闭；本修正不构成UI或exclusion验收。
