# Windows crossapp StageB — CC independent review + actual execution — 2026-10-01

- 执行身份：CC（真实 Windows acer-win，Session 0 纯 console；仅文件/console 工作，经既有 approved bounded runner 调度；无 GUI/应用/Host/浏览器启动、无截图/监听、无权限/防火墙/全局改动；未执行任何页面 JS/渲染；root Rust/Mac/Linux 未触碰；无 commit/push/worktree）。实际模型 glm-5.3-flash（报告内任务文本中的 expected-model 字符串仅为生成内容，不作为模型证据）。
- 基线：作者 r3-final SOURCE_FROZEN。manifest `frozen-source-r3-sha256.txt`（自身 SHA256=`200a9217…`，与协调者给定一致）+ `windows-crossapp-stage-b-source-r3.tar.gz`（SHA256=`b0daca75…`，一致）。未使用 r1/r2。
- 结论：**5 步全部符合预期（pure console）。** 本地+远端 24/24 哈希前后一致；原 49 主测试 **Ran 49 / OK（0 fail/error/skip）**、新增 `tests/windows_crossapp_pack_stage_b.py` **Ran 13 / OK**；prepare CLI native=0（pending/gui_verified=false/6 cases）；checker CLI native=0（6 例全部 `needs_evidence`，gui/ownership/gate 全 false）；对同一已存在 pack 重复 prepare **native=2** 明确拒绝且不覆盖（74 文件全部创建于 negative 步骤开始之前，创建数 after=0）。零源码修改，无 rerun-for-green。

## 1. 独立 review（spec + quality，执行前，无 blocking issue）

- **StageA 输入保留**：API.md、drag.md、windows-focus.md、runner ps1/Runner.cs、原 `tests/windows_crossapp_pack.py`（49 方法断言未动）SHA 与 StageA 冻结清单逐一相同；`crossapp_artifacts.py` 由 stub 转具体实现（预期变更）。`preserved-stage-a-inputs.json` 与实际一致。
- **5 API 真实现**：paths（NEW root 校验/UNC/device/drive-relative/遍历/ADS/保留名/末尾点空格/reparse lstat 逐组件/映射盘 DriveType fail-closed；resolve 不用 resolve() 以免跟随 reparse）；pack（exclusive mkdir、父目录身份前后复核、O_EXCL 写、失败保留不回滚）；io（读写前后身份+mtime 复核、O_NOFOLLOW、单硬链接、限 64/256/512KiB、目录条目≤128）；artifacts（严格 UTF-8/BOM-UTF16 解码，UTF-32/裸 UTF-16→unknown，无 normalize/trim，原字节 hash 保留；oracle 与确定性契约不一致→unknown 而非信任）；browser_check（有界事件序列、first-attempt form 保留、drag start<motion<drop<end、isTrusted/self-report 永不升级 GUI）。
- **2 CLI**：prepare/check 均 supervisor-only、参数仅 --root、失败 stderr JSON + exit 2、prepare 成功输出 pending/needs_preflight/cases=6；check exit 0 仅表示诊断输出。
- **整合点核对（r3）**：crossapp-05/06 wrapper 均为 "ONE shared Describe/Open"（同一 session，不两个）；原任务全文嵌入（BEGIN/END 标记之间逐字不变，测试断言 embedded==original）；原任务控制收尾、最终 computer_close 恰一次、**Close 后零额外工具**；crossapp-06 在嵌入第 1 步后、任何 target 输入前插入 pause→核验真实 paused 回复→resume→核验回复→fresh Observe 前奏；无 held-cancel、focus-10 Close 后无工具。静态契约由新增 `test_focus_wrapper_has_ordered_same_session_pause_resume_prelude` 覆盖（本轮实际运行通过）。
- **诚实边界**：模块文档与 manifest `safety_boundary` 明确"非原子 TOCTOU 保护，需私有静止 root+parent"；未知外来文件不删除（checker mismatch 时 "nothing deleted"）；所有权字段全 null/needs_preflight；protected_user_pids=[24332]；05/06 `pack_nonce_is_fixture_nonce=false`、binding 需 supervisor 显式绑定、不强求相等。
- **Browser 静态页面**：course.html/css/js 本轮**未渲染、未执行 JS**——仅验证了 prepare 将其内嵌渲染进 pack 页面资产与 checker 对导出 JSON 的离线校验；不把静态 HTML 关键字检查当作交互通过。

## 2. 冻结 staging

- 24 文件按相对路径 tar → scp → **新唯一 TEMP**（布局保留，24 文件）：
  `C:\Users\Administrator\AppData\Local\Temp\computer-crossapp-green-cc-aeb955ff79eb4b1c846b737d8610b04e`
- 远端 Get-FileHash 对照 r3 manifest：**24 OK / 0 非 OK**（`xa-b-hashverify.txt`）；运行后复查再次 **24 OK / 0 非 OK**（`xa-b-hashrecheck.txt`）——测试过程未改任何源码。
- Python：真实安装 3.12.10 路径（非 Store 占位符）；runner summary `executable_sha256=4d6f5f81…`，runner 两源 SHA=`ad2a607c…`/`aced18e6…` 与冻结一致。

## 3. 5 步执行（on-disk `xa-b-master.ps1`，每步独立 config+evidence leaf，父预建/leaf 原子保留；`PYTHONDONTWRITEBYTECODE=1` 仅父进程环境）

pack 目录：`<root>\sbout\pack-3dea080d76704e53800845e3ad520dee`（NEW，prepare 前不存在）。

| 步骤 | 命令 | timeout | native | timed_out | 结果 |
|---|---|---|---|---|---|
| 1 pack49 | `-B tests\windows_crossapp_pack.py -v` | 180 | **0** | False | **Ran 49 tests / OK**（stderr 5830B） |
| 2 pack-b13 | `-B tests\windows_crossapp_pack_stage_b.py -v` | 180 | **0** | False | **Ran 13 tests / OK**（stderr 1670B） |
| 3 prepare | `-B scripts\prepare-windows-crossapp.py --root <pack>` | 90 | **0** | False | stdout JSON：`state=pending, gui_verified=false, gate=false, cases=6, ownership=needs_preflight` |
| 4 check | `-B scripts\check-windows-crossapp-artifacts.py --root <pack>` | 90 | **0** | False | 6 例全 `needs_evidence`；report `gui_verified/ownership_verified/gate` 全 false |
| 5 prepare-negative | 同 prepare 指向同一已存在 pack | 90 | **2** | False | stderr：`"root already exists; never overwrite or reuse it"`；outcome=native_failure 为**预定拒绝**，非意外 RED |

全部 `root_exited`、双输出 drain 完整、无截断。真实状态取自各 summary.json `native_exit_code`，未用 harness/CLI exit 冒充。

### 生成 pack 实际内容（74 文件，从 owned 目录读回）

- manifest：`schema=windows-crossapp-pack-v1`、`pack_id=95510381…`、`gui_verified=false`、`action_trial_gate_satisfied=false`、`status=pending`、`protected_user_pids=[24332]`。
- **6 例身份**：crossapp-01..06 nonce/run/trial 各 **6 unique**；全部 `status=pending`、`ownership.status=needs_preflight`、`owned=null`（窗口身份 null，不预认）。
- crossapp-01：空 nonce 命名资产文档（0 字节）与结果路径分开；checker 对空资产判 `needs_evidence`（非结果）。crossapp-02：**3 个独立 trial_id**（`3c7ca695…/474cc5f8…/b2f9845f…`）全 `needs_evidence`、`requires_fresh_observation=true`，非重复读同帧。crossapp-03：nonce 目录 64 个 source 文件+空 destination（清单见 `pack-inventory-after-negative.txt`）。crossapp-04：离线页资产 + 专属 profile/download 目录（仅模板，**未启动浏览器**）、`needs_supervisor_configuration/confirmation`。
- **native 复用**：x05 `suite=drag, source_sha256=85a5f796…`；x06 `suite=focus, source_sha256=128b9e6b…` —— 与 StageA 冻结清单中两任务文件 SHA 完全一致（原任务嵌入不变）；`binding_status=needs_preflight`、外层 pack nonce ≠ fixture nonce、实际 PID/创建时间/HWND 绑定字段全 null。

## 4. 非 negative 验证（步骤 5 不覆盖证据）

- **文件创建时间核对**：74/74 pack 文件 CreationTimeUtc 均早于 negative 步骤 `root_start_utc=17:57:44.077Z`；negative 期间/之后新建 **0** 个。
- 文件清单+SHA256 快照 `pack-inventory-after-negative.txt`（10024B，74 行）。API 层不覆盖另由步骤 2 的 `test_file_write_never_overwrites` 实际运行证明。

## 5. Wrapper 错误记录

本轮 wrapper（stage/hashverify/master/collect/verify2/recheck/residual）**一次成功，零首败**；无上轮四类错误复发（无引号内联、无 scp `+` 路径、无 Join-Path -Exclude、无预建 leaf、无 `'^('` 正则）；无 cmd `%ERRORLEVEL%` 权威状态。

## 6. 残留与完整性

- 哈希复查 24 OK / 0 非 OK（§2）。
- 残留：read-only `Win32_Process` 按 owned 根路径过滤 CommandLine（无 kill）：**0 进程**（`xa-b-residual.txt`）。Notepad24332 未触碰。

## 7. 归档

`.agents/runs/windows-crossapp-stage-b-cc-20260930/`：
- `evidence/{pack49,pack-b13,prepare,check,prepare-negative}/`：各步 summary.json/stdout.log/stderr.log/config（SHA 见 `archive-sha256.txt`）
- `xa-b-pack.tgz`（实际生成 pack 全量读回，SHA `b29b8fad…`）、`pack-inventory-after-negative.txt`
- `xa-b-remote-final.tgz`（远端 staged 根终态，SHA `6e5057ae…`）
- `collect.txt`/`verify2.txt`（6 例状态、identity、3 trial 唯一性、check 报告全文）、双份 hash-verify/recheck、residual
- wrapper 全套脚本 + 冻结 tar/manifest 副本。远端 TEMP 根保留。目录内 task.md/transcript.jsonl 为并发工作区文件，非本轮产物，未触碰。

## 8. 状态与边界（明确区分，不隐瞒）

1. **纯自测（本轮回，已通过）**：49+13 unittest 均为 synthetic owned fixture/静态契约；测试创建的模拟文件（如伪造 browser 导出）全部标记 untrusted/unknown 导向，未伪造任何 GUI pass。
2. **真实文件准备（已完成，非 GUI）**：prepare 生成真实 owned pack（74 文件），check 只读诊断 6×needs_evidence；exit 0 ≠ GUI pass。
3. **浏览器 JS 未测试**：course.js 未渲染/未执行；页面仅静态资产校验。
4. **六例 GUI 全部未执行**：无 Notepad/Calculator/Explorer/browser/drag/focus 运行；`gui_verified=false`、`action_trial_gate_satisfied=false`、ownership null 保持；十次有效 trial 门禁未满足。
5. StageA 首轮 49 用例 RED 历史保持不变；本轮 49 测试转绿由 StageB 实现达成，**不表示整个 computer 项目完成**；未运行其它 suite/firmware/root Rust。
6. 不声称抵抗恶意并发 TOCTOU（作者文档化边界：root+parent 需私有静止）。
7. 未改产品/测试源码/harness；未 commit/push/worktree；实际模型 glm-5.3-flash。

**STOP：StageB 五步 console 验证全绿已报告。所有 GUI/supervisor 绑定/真实 trial 证据仍 pending，由协调者掌握。**

## 9. 协调者复核后的追加 hashproof（不重跑原五步）

- **承认缺口**：最初 negative 非覆盖证据只用了 CreationTime 未变 + after-only 清单，**创建时间不能证明已有文件内容未被覆写**。现补独立追加验证（仅 runner 编排 + 只读 hash，非 rerun 碰绿）：
  1. **before**：对同一 owned pack（`pack-3dea080d76704e53800845e3ad520dee`，从 prepare stdout/config 精确读取）只读捕获全部文件相对路径+length+SHA256+目录清单，74 文件，存 pack 外 NEW `sbout\hashproof\inventory-before.txt`（SHA256 `d6cec8ba…`）。
  2. 原 r3 prepare CLI 对同一 existing root 经同一冻结 runner（runner 源 SHA `ad2a607c…`/`aced18e6…`）再执行一次，NEW leaf `hashproof-evidence\prepare-negative-hashproof`，timeout 90：**native=2**（预期拒绝，非异常 RED）、`timed_out=false`、`root_exited`、stderr 196B（同 "root already exists" 拒绝消息）、stdout 0B。
  3. **after**：立即同格式重捕 `inventory-after.txt`——与 before **SHA256 完全相同（`d6cec8ba…`，字节级一致）**；逐项比较 `comparison.txt`：74/74 条目（hash+length+相对路径）与目录清单完全一致，only-in-before=0、only-in-after=0，**IDENTICAL: True**。
  4. 源码前后复查 **24 OK / 0 非 OK**；未启动任何 GUI/应用；Notepad24332 未触碰。
- 归档：`.agents/runs/windows-crossapp-stage-b-cc-20260930/hashproof/`（before/after/comparison、该 runner 的 summary/stdout/stderr/config、inventory/negative/compare 脚本、wrapper 首败记录 `wrapper-errors.txt`：param 行与 `\\` 正则两次 heredoc 吞字符失败，原样保留）。
- **计数更新**：runner 总数现为 **6**（原五步全部保留不重跑）；unittest 总数仍为 **62（49+13）**，本轮未新增任何测试。

## 10. 源码解释纠正（按实际源码/原测试结果）

1. **裸 UTF-16 并非"自动检测并一律 unknown"**：`check_notepad_bytes` 只按 BOM 分派——UTF-8 BOM→`utf-8-sig`、UTF-16 LE/BE BOM→按对应 UTF-16 解码；**其余一律按 strict UTF-8 解码**。无 BOM 的 UTF-16-LE 字节可能是合法 UTF-8 文本，结果是 `mismatch` 或（字节非法时）`unknown`，并非固定 unknown。（StageB `test_utf32_and_bomless_utf16_not_equivalent` 只断言 `state ≠ artifact_match`，与该语义一致。）
2. **合成有效 browser 记录不是"全部 unknown"**：结构完整的 browser 导出（schema/identity/有界事件序列/first-attempt 保留/drag 顺序/最终值匹配确定性契约）经 `check_browser_export` 可判 **`artifact_match`**——实际运行 `test_synthetic_browser_match_never_proves_gui` 即如此；但它仅为 application data，`gui_verified` 恒 false、`application_data_only=true`。只有畸形/缺失/超限/身份不符等才 unknown/mismatch。原 §1 中"自报不可信"指 isTrusted/download 等标志永不升级 GUI 证明，不指状态必为 unknown。
