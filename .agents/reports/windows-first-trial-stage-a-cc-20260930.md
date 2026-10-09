# Windows first-trial accounting — Stage A CC 真实 RED 验证

日期：2026-09-30 UTC（远端执行记录 2026-09-30T19:43:37Z）。执行者：CC（Claude CLI + glm-5.3-flash 别名）。
依据 SOURCE_FROZEN 报告 `.agents/reports/windows-first-trial-stage-a-sol-20260930.md`
（冻结 2026-09-30T19:34:27Z）与计划
`docs/superpowers/plans/2026-09-30-windows-first-trial-accounting.md`。

## 结论

**真实能力缺失 RED 成立。36/36 需求方法全部 FAIL（含 subTest 共 120 个 FAIL
条目），0 ERROR，0 PASS。失败原因唯一：两个 Stage A 显式 NotImplementedError
stub 被测试 wrapper 转为 unittest FAIL——`MISSING_CAPABILITY[build_manifest]`
19 条（PlanRequirements 全部 8 方法的 subTest）、`MISSING_CAPABILITY[summarize]`
101 条（SummaryRequirements 全部 28 方法的 subTest）。无 import/权限/文件/环境
错误被误计；unittest 终态 `Ran 36 tests in 0.081s / FAILED (failures=120)`，
Python native exit code **1**。未实现/未修复任何断言；未做 StageB。**

## 静态核验（执行前，未轻信作者声明）

- 本地四源文件 SHA256 与冻结报告逐一相符（API.md `dce62b6d…`、`__init__.py`
  `8502c470…`、canonical-fixture.json `6834b40f…`、测试 `86a6b5f0…`）。
- 冻结 tar `stage-a-source.tar.gz` SHA256 = `062d755b207e649517767eb68b86a2cb1
  bed1218e03a60d16d962a2090352324`，`SOURCE_SHA256SUMS.txt` SHA256 =
  `61cbee297fa4c3846781d906ffd06d8946c75a88e6a45429054305b512fe3991`，均与报告
  相符。tar 含 19 文件（4 新源码 + 15 只读输入，含冻结 runner 两文件）。
- 测试实体性审查：summary 测试 setUp 使用独立固定 canonical-fixture.json，
  **不经 build_manifest 前置阻断**（仅 tampering 测试中以 fixture 直接构造
  manifest）；断言触及 plan 生成、ID 稳定、分类/variant、去重/冲突、重试不
  提升、严格类型与界限、防篡改、两模式分离等实质语义，非仅 import 或合成
  GUI 通过。两个缺失函数均被真实调用触达。判定：无阻塞性合同/测试缺陷，
  可执行真实 RED。

## Windows 执行（单次）

- 远端身份 `node1\administrator`（SSH acer-win）；Python 已存在且未改动：
  `C:\Users\Administrator\AppData\Local\Programs\Python\Python312\python.exe`
  → `Python 3.12.10`。
- NEW TEMP：`C:\Users\Administrator\AppData\Local\Temp\windows-first-trial-stage-a-cc-20260930-r2\`
  （首次尝试目录 `…-cc-20260930\` 因下述 wrapper 缺陷未执行任何测试，保留为
  wrapper 失败证据）。目录内 `source\`（tar 解包，含 scripts/windows-test-runner.ps1
  与 scripts/windows-test-runner/Runner.cs 兄弟布局）、`config\`、`evidence\`
  （仅父目录预建；叶 `evidence\stage-a-run\` 由 runner 原子预留创建）。
- 远端 19/19 文件 SHA256 全部 OK（`hash_bad=0`，见 `evidence/remote/remote-file-hashes.txt`）。
- 配置（config schema 逐字段）：
  `executablePath`=Python312 路径、`workingDirectory`=`…\source`、
  `evidenceDirectory`=`…\evidence\stage-a-run`、
  `arguments`=`["-B","tests\\windows_first_trial_accounting.py","-v"]`、
  `timeoutSeconds`=180。
- **单次执行**，同一 SSH 进程等待至权威终态（未重跑）。runner summary 关键字段：
  - `native_exit_code` = **1**（CLI terminal≠通过；此处为 unittest FAILED 的真实退出码）
  - `outcome` = `native_failure`（预期 RED）
  - `timed_out` = **false**（远未触及 180s，进程 0.081s 内自然终态）
  - `cleanup_state` = `root_exited`；无 kill
  - stdout drain 完整（0 字节；unittest 输出走 stderr）；stderr drain 完整
    242,808 字节、`stderr_truncated=false`
  - `root_pid` 9632、`root_session_id` = 0、identity 源
    `exact_absolute_ProcessStartInfo_FileName`；runner PID 8380
  - `runner_source_sha256`/`runner_module_sha256` 与冻结一致（ad2a607c…/aced18e6…）

## 结果分解

- `Ran 36 tests in 0.081s`，`FAILED (failures=120)`；**0 ERROR**。
- 36 个唯一测试方法全部 FAIL（PlanRequirements 8、SummaryRequirements 28），
  无任何方法 PASS 或 SKIP。
- 120 = subTest 计数：19（Plan，全部 `MISSING_CAPABILITY[build_manifest]`）+
  101（Summary，全部 `MISSING_CAPABILITY[summarize]`）。逐 traceback 核验：
  异常链仅 `NotImplementedError → self.fail("MISSING_CAPABILITY[…]")`，
  无 ValueError/ImportError/权限等其它类型混入；合成记录仅作统计输入、
  显式 synthetic 身份，无 GUI 证据宣称。
- 计数与 API 源码一致（两个 stub 均显式 `raise NotImplementedError`），
  非任意目标数。

## Wrapper 缺陷（与原生测试失败严格区分，单独记录）

- 尝试 1：我编写的 wrapper 使用 `Windows\Microsoft\WindowsPowerShell\v1.0`，
  该主机不存在此路径（正确路径为 `System32\WindowsPowerShell\v1.0`，与既往
  批次一致），`& $ps` 从未启动 runner。`runner_exit=0` 是 `$LASTEXITCODE`
  假象，**不是测试通过**。该次无任何测试执行、无任何 runner 产物（evidence
  目录为空）。证据：`remote-run.attempt1.stdout/.stderr`。修正 wrapper 后在
  新 TEMP 目录重新 stage（再次 19/19 哈希核验），测试仅执行一次。

## 边界

- 纯 Python/文件系统验证；无 app/Host/GUI、无 socket、无输入注入、无权限/
  防火墙/全局改动、无 Notepad PID24332 或任何进程操作、无 worktree/commit/
  push/reset、无源码写入。临时写入仅限本批次 TEMP 目录。
- 本结果是纯 RED 能力缺失证据：不构成 GUI 验收、不构成 120 slot 实际执行、
  不构成十次操作验收；`gui_verified`/`action_trial_gate_satisfied` 语义留待
  Stage B。未做 StageB 实际 CLI 调用。

## 冻结复核（执行后）

- 本地冻结 tar 与 SOURCE_SHA256SUMS 再算哈希与报告一致（062d755b…/61cbee29…）。
- 远端 transfer-tar.sha256（062D755B…，大小写差异为 Get-FileHash 格式）与
  19 文件远端哈希清单与冻结清单逐值一致（仅排序/大小写格式差异）。
- 本轮仅写本报告与 `.agents/runs/windows-first-trial-stage-a-cc-20260930/`；
  被测源码与测试零改动。

## 索引

`.agents/runs/windows-first-trial-stage-a-cc-20260930/`：
- `stage-and-run.ps1`（最终 wrapper；首次缺陷版本 diff 见报告正文）
- `remote-run.attempt1.stdout/.stderr`（wrapper 失败证据）、
  `remote-run2.stdout/.stderr`（正式单次运行全量输出）
- `evidence/remote/stage-a-run/`：runner 原生 `summary.json`、`identity.json`、
  `config.json`、`stderr.log`（242,808 字节全量测试输出）、`stdout.log`（0 字节）
- `evidence/remote/remote-file-hashes.txt`（远端 19 文件哈希）、
  `transfer-tar.sha256`、`stage-a-run.config-copy.json`（实际使用的 config）
- `post-freeze-recheck-local.txt`、`scp.stderr`、`scp2.stderr`、`dl.stderr`

STOP —— Stage A RED 证据交付完毕，等待作者 Stage B 与后续许可。
