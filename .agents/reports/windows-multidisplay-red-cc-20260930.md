# runtime::runtime::tests::multidisplay_contract_red — Windows root baseline RED gate（CC 报告）

日期：2026-09-30。执行者：CC（实际模型 glm-5.3-flash）。依据 SOL 报告
`.agents/reports/windows-multidisplay-red-sol-20260930.md`（SOURCE_FROZEN 后）执行。
**未做任何生产/测试源码修改**；Mac 上仅交叉构建，Windows 上仅执行精确模块 filter。

## 1. 源码冻结核对（运行前 + 运行后）

```text
bb8a325981f6ed5f5596f8502fdd2aa3453ee6752339aebbeb938895290bdba5  src/runtime/runtime/tests.rs
79c30152b52c3f742cf974f81c5a2471e10539391dbb967ed796eee3c26cb3e1  src/runtime/runtime/tests/multidisplay_contract_red.rs
```

与 SOL 冻结值**完全一致**；任务结束后二次核对**逐字节不变**。本批次未改动任何
`src/`、Cargo 文件、fixture、renderer 或既有产物目录。

## 2. 测试授权审查（只读）

`multidisplay_contract_red.rs`（229 行、6 个测试）逐个确认：仅使用现有
`Runtime::new/call`、`FakeBackend::new/set_geometry`、`tool_definitions`、
`decode_png` 内存 PNG API。无 `#[should_panic]`/ignore/skip/反向断言。唯一
backend 是内存 FakeBackend（合成显示器 `mock-display-negative-origin`，
bounds(-640,120,640,360)，PNG 320×180），无 DesktopBackend/WindowsDisplay/GDI/
native attach/输入注入/网络/renderer/worker/文件 fixture。授权范围内，无需桌面
交互槽（实际也未申请）。

## 3. Windows 交叉构建（Mac，仅 --no-run）

新独立输出目录（未触碰旧 target）：

```bash
source .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh
export CARGO_TARGET_DIR="$PWD/.agents/runs/windows-multidisplay-red-cc-20260930/target"
cargo test --target x86_64-pc-windows-msvc --locked --no-run --lib --message-format=json
```

构建 exit **0**。从 compiler-artifact（`profile.test=true`、
`target.name=rpa_computer`）精确选出唯一 lib test exe（未 glob）：

```text
.agents/runs/windows-multidisplay-red-cc-20260930/target/x86_64-pc-windows-msvc/debug/deps/rpa_computer-3769e6ba7e450727.exe
本地 SHA256: d036aff39988f786b2ac344daf0abe32c15856eee2c3fa244ef06ba616bb806d  (PE32+ console x86-64)
```

## 4. Windows 执行（acer-win，SSH 调度；无 native 调用，SSH session 合法）

新独立 TEMP：`C:\Users\Administrator\AppData\Local\Temp\md-red-cc-20260930-r1`。
exe 上传后双端 SHA256 **一致**（`D036AFF3…B806D`）。

### r1（首次运行，完整 RED 证据，数值 exit 缺失）

`md-red-runner-r1.ps1`：`Start-Process -PassThru` + 等待前身份落盘 +
`WaitForExit(120s)`。测试自退出（0.08s），**6 选中 / 6 failed / 0 passed /
250 filtered out**。但 `ExitCode` 为 null（上批已知 PowerShell 坑；null 不记为
0，r1 证据原样保留，未覆盖）。

### r2（数值 exit 补采，启动即失败，无任何文件产生）

`md-red-runner-r2.ps1` 用 `ProcessStartInfo.ArgumentList`——Windows PowerShell
5.1（.NET Framework）无此 API，`InvokeMethodOnNull`，**exe 从未启动**，r2 日志/
状态文件不存在（仅 runner 脚本留档）。

### r3（最终有效数值证据）

`md-red-runner-r3.ps1`：原生 `System.Diagnostics.Process`（`UseShellExecute=$false`
+ `Arguments` 字符串 + 保留句柄），流式双文件、等待前身份记录、120s 有界监督
（超时需 pid+exe+creation-ticks+session 四元复核才可杀）。

**实际执行一次，结果（r1 与 r3 完全一致）：**

```text
running 6 tests
test ...::describe_declares_native_unit_separately_from_image_coordinates ... FAILED
test ...::describe_full_generation_is_stable_but_distinguishes_fresh_authorities ... FAILED
test ...::describe_lists_backend_display_facts_not_static_primary ... FAILED
test ...::explicit_primary_is_accepted_by_runtime_and_advertised_open_schema ... FAILED
test ...::observe_generation_matches_describe_and_survives_unchanged_recapture ... FAILED
test ...::observe_reports_selected_region_using_actual_resized_png_and_native_bounds ... FAILED
test result: FAILED. 0 passed; 6 failed; 0 ignored; 0 measured; 250 filtered out; finished in 0.29s
```

- **数值 native exit = 101**（`md-red-test-status-r3.json` 的 `native_exit` 字段，
  由保留句柄的 Process.ExitCode 读取；非 cmd 回显、非 Start-Process 晚取）。
- elapsed 0.297s，自退出，远小于 120s 预算，未触发身份杀。
- 选中 6 项（filter 无 `--exact`，模块前缀），与 SOL 预期一致；**无意外 pass**。
- r1 与 r3 是同一 filter 的两次真实调用（r1 完整 RED、r3 数值 exit 采集），
  两次输出均保留。

## 5. 每项首败断言（真实 RED 暴露的契约缺口，均交 root owner，本 tester 不修）

| 测试 | 首败位置 | 缺口 |
|---|---|---|
| `describe_lists_backend_display_facts_not_static_primary` | tests/multidisplay_contract_red.rs:67 | describe 无 `displays[]` 数组，不发布 backend 显示器事实（ID/is_primary/native_bounds） |
| `describe_declares_native_unit_separately_from_image_coordinates` | :79 | 无独立 `native_unit` 声明（fake 合法取 `points`/`physical_pixels`） |
| `describe_full_generation_is_stable_but_distinguishes_fresh_authorities` | :55 | 无完整 `topology_generation`（要求同 authority 稳定、独立 authority 可区分） |
| `explicit_primary_is_accepted_by_runtime_and_advertised_open_schema` | :138 | Runtime 接受 `display:{kind:"primary"}` 但 open schema 无 `properties.display`——"静默忽略"被防假绿断言拦下；8 tools 保留 |
| `observe_reports_selected_region_using_actual_resized_png_and_native_bounds` | :185 | 旧 PNG/尺寸部分通过（160×90 downscale 正常）后，`mapping_regions[]`/`selected_display_ids[]` 缺失 |
| `observe_generation_matches_describe_and_survives_unchanged_recapture` | :55 | 观察 reply 无 `topology_generation`（首个 observation 处失败，不接受 null==null） |

全部失败是**合法契约 RED**（断言缺失字段/schema），非编译失败、非缺 asset、非
invocation 破损。SOL 预期 exit 101 与实际一致。

## 6. 覆盖限制声明

- FakeBackend 平台 `fake`，单合成显示器；本 RED **不构成**真实多屏枚举、DPI、
  GDI、热拔插、drag generation 或任何 GUI/multiscreen 行为证据。
- 未验证真实 Windows backend 必须为 `physical_pixels`（属 GO 后新枚举接口测试）。
- 字段拼写是 SOL 报告声明的候选投影，需 root owner 在 Task 2 GO 前统一。

## 7. 残留与用户进程核查

- 测试进程自退出；运行后按 exe 路径/进程名扫描 acer-win → **0 残留**。
- 未注册任何 scheduled task；未杀任何进程；无 GUI/capture/输入/网络调用。
- 用户 Notepad pid **24332** 运行前后均在（ProcessName=Notepad，MainWindowTitle
  为空且无变化），未触碰。
- TEMP 仅本 run 自有文件（r1+r3 证据 + 3 个 runner 脚本 + exe），未删除、未覆盖
  任何先前批次产物。

## 8. 产物索引

- run 目录：`.agents/runs/windows-multidisplay-red-cc-20260930/`
  - `compiler-artifacts.jsonl`、`logs/build.stderr.log`、`logs/build.exit`（=0）
  - `logs/local-exe-hash.txt`（d036aff3…）
  - `md-red-runner-r{1,2,3}.ps1`（本地原件；r1/r3 已上传，r2 上传后远程启动失败）
  - `logs/win-runner-{,-r2-,-r3-}stderr.log`
  - `logs/win/r3-evidence-dump.txt`（status+stdout+stderr+Notepad+TEMP 清单）
  - `logs/win/win-evidence-hashes.txt`（远程全部证据 SHA256 + 残留=0）
- Windows TEMP：`C:\Users\Administrator\AppData\Local\Temp\md-red-cc-20260930-r1`
  - `md-red-{stdout,stderr}-r{1,3}.log`、`md-red-test-{identity,status}-r{1,3}.json`
  - `rpa_computer-3769e6ba7e450727.exe`（SHA256 见上，双端一致）

## 结论

**Task 1 baseline RED 成立**：精确 filter 选中 6/6，6 failed、0 passed、
250 filtered out，数值 native exit **101**，失败均为真实缺失契约断言。源码冻结
hash 前后一致，无任何生产改动，无残留。交协调者判定 Task 2–4 GO；本 tester 到此停止。
