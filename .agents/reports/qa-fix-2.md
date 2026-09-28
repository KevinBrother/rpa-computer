# qa-fix-2 — 真实 Claude 验收审计 + 脚本剩余缺陷修复报告

日期：2026-09-24 ｜ 作者：QA repair worker qa-fix-2
依据：`.agents/tasks/qa-fix-2.md` + `.agents/reviews/continuation-review.md` QA 条目 1–6 +
qa-fix-1 终报（`.agents/reports/qa-fix-1.md`，其会话 exit0 已经协调者确认）。
范围：scripts/**、tests/mcp_protocol.py、examples/acceptance/**、README.md、本报告。
未改 src/Cargo/tests 下 Rust 测试（peer 占用），未 commit/push/worktree，未运行 GUI，
未启动任何 Host。acceptance-fixture/** 为 visual-fixture worker 独占，未触碰。

---

## 1. 真实 Claude 验收运行器：可执行 + 可审计（continuation-review QA #1，任务优先级 1）

qa-fix-1 的启动器只审计**假 argv**，无 stream-json/verbose、无证据捕获、无真实工具
清单校验。本次重写 `scripts/run-blackbox-claude.sh` 并新增
`scripts/audit-claude-transcript.py`：

**启动 argv（print 验收路径，单一事实源 `build_claude_args`）**：
- `--tools ''` + `--allowedTools` 仅 8 个 `mcp__computer__computer_*`；
- `--strict-mcp-config`、`--disable-slash-commands`、`--settings '{"disableAllHooks":true}'`；
- `--system-prompt` **替换式**干净提示（原为 `--append-system-prompt`，review 明确要求 clean
  system-prompt 而非 append）；
- `--setting-sources user`：用户自己的模型/认证设置原样保留，release 目录附近的项目/本地
  设置不加载（**未**加 `--model`/auth 相关 flag，用户模型与认证不动）；
- `--output-format stream-json --verbose --no-session-persistence --max-turns N`
  （默认 12，可 `--max-turns` 调整；`--max-seconds` 默认 600 的墙钟预算由看门狗执行，
  只 TERM/KILL **本次运行的精确子 PID**，绝不按名称匹配进程）；
- 无任意 trailing claude 参数（exit 2）、无提权 flag。

**任务文本**：必须非空且明确要求基于截图/观察作答（grep 截图|screenshot|观察|observe），
否则拒绝（exit 1）。任务位于源码树内仍拒绝。

**证据捕获（在源码树外）**：print 模式把真实转录写到
`<release>/evidence/<run-id>/transcript.jsonl`、stderr 到 `stderr.log`、退出码到
`exit-code.txt`，目录 0700。token 从不传给 claude（MCP 子进程自读 token 文件），
转录不含凭据；若未来出现凭据，0700 ACL 是缓解层。

**转录审计（fail-closed，可对真实转录重复执行）**：运行结束后启动器自动执行
`audit-claude-transcript.py <transcript> --expect-server computer --max-turns N --max-seconds S`：
- init 事件恰好一个，`init.tools` **恰好**等于 8 个 computer_* MCP 工具（多一个内建
  工具即 FAIL）；`mcp_servers` 恰好 `{computer}` 且 status connected（缺失/多余/未连接即 FAIL）；
- 全部 assistant tool_use 必须是 computer_*（出现 Bash/Read 等即 FAIL）；
- tool_result 中的权限拒绝文本、任何 `hook_response` 事件、`result.is_error`/
  `error_during_execution`、num_turns/duration 超预算 → FAIL；
- 无 init、空文件、零工具调用、无 result → FAIL。
通过审计 ≠ GUI 任务正确（脚本与 README/夹具文档均显式声明；真值比对属协调者）。

**证据**（本机真实执行，非静态）：
- `--verify-only`：22 项 argv 固定位置审计通过（含新 flag 位置）；
- 端到端沙箱 print 运行（假 claude 进程**真实发射 stream-json 转录**，经
  sandbox-exec + 负向探针 + 证据捕获 + 审计）：发布目录在 `$HOME` 下 →
  `AUDIT OK (17 checks)`、rc=0、证据三件齐全；发布目录在临时根下 → **fail-closed**
  rc=126（验收 profile 拒绝临时根，转录为空、审计 FAIL、运行不计数）；
- 流氓转录（tool_use=Bash，进程 exit 0）→ 运行仍 FAIL（"unexpected tool call"）。

## 2. 沙箱 deny 规则修复（continuation-review QA #2）

- **regex 缺斜杠**：原 `"^%srpa-computer-.*"`（TMPDIR 以 / 结尾时侥幸可用，否则不匹配）。
  现为 `"^<canonical-root>/rpa-(computer|core)-[^/]*"`，根先 `pwd -P` 规范化并去重
  （/tmp 与 /private/tmp canonical 相同只写一条）。
- **已知源码副本**：`/tmp/rpa-core-shim`（确认含 Cargo.toml+src，17:56 旧 transport 工作
  残留）经 canonical 后以 `(subpath ...)` 拒绝；只在验证确含项目源码时才列入，
  **未删除任何目录**（未知 scratch 不触碰）。profile 构建对 SOURCE_DIR 拒绝 `"`/`\`
  元字符（防 sandbox 语法注入），所有路径 canonical 且安全引用。
- **print 验收 profile** 额外拒绝临时根本身（发布目录生产上永不位于 /tmp；位于则
  fail-closed——已实测）。探针 profile 与运行 profile 共享**逐字节相同**的源码/副本
  deny 段；探针用无临时根 deny 的版本以便其 4c 暂存探针可读。
- `sandbox-probe.sh` 增强：4b 对 profile 中每个非源码树 subpath deny 做读探针
  （实测 /private/tmp/rpa-core-shim 下 .rs 在沙箱内不可读）；4c 在自有
  `rpa-computer-probe-regex.*` mktemp 目录验证 regex deny 真实生效（实测）。独立运行
  的探针 profile 现在同样带 scratch regex + 已知副本 deny，输出明确列出覆盖范围。

## 3. Windows 任务/进程身份加固（continuation-review QA #3/#4，任务要求 3）

`windows-start-host.ps1`：
- **任务碰撞 fail-closed**：已存在同名计划任务时，只有其动作数==1 且
  Execute==本部署 `run-host-logged.cmd` 且 Principal.UserId==交互用户才重注册；
  否则 `Write-Error` 退出 1，**绝不删除/替换无关任务**。
- **token ACL 按 SID**：交互用户解析为 SID（NTAccount.Translate），allow ACE 全部
  Translate 成 SID，必须是 {用户 SID, S-1-5-18 SYSTEM, S-1-5-32-544 Administrators}
  子集，否则启动前拒绝；不再使用 Everyone/BUILTIN\Users 显示名匹配（区域设置脆弱）。
  错误信息只含路径/主体，**token 内容永不打印**。
- host-instance.json 增加 `creation_date_utc`、`interactive_user_sid`。

`windows-stop-host.ps1`：
- `-TaskName` 不再能覆盖为记录外任务：与记录不一致即 exit 1；
- 停任务前复核**活任务**的动作路径与主体仍与记录一致（否则只报告、不动）；
- 停进程前复核 PID 的 exe 路径**与创建时间**（>2s 偏差=PID 复用，拒绝）；
- 无名称模式杀、无 pkill。

`deploy-windows-test.sh`：
- **manifest 不含自身**：`find ... ! -name MANIFEST.sha256`（原重定向先建文件后 find
  会把半成品 manifest 哈希进去——review QA #4）；
- **远端 PowerShell 全部改 `-EncodedCommand`**（base64/UTF-16LE）：消除多层嵌套引号；
  部署后对两个 .ps1 做远端 **PSParser 解析检查**（不执行、不启动任何 Host/GUI）；
  token ACL 远端逻辑同样改 SID 白名单（控制台 explorer 属主 + SYSTEM + Administrators）；
  token 生成后即 `unset`，错误路径不含 token；`ps_remote` 辅助函数对脚本文本中的
  `$` fail-fast（防本地 bash 展开产生畸形远端命令，部署脚本内的 PS 文本全部单引号静态）。
- **远端实测（ssh acer-win，仅解析/目录/哈希，未启动 Host/GUI）**：两个 .ps1 远端
  `PSParser::Tokenize` → `PARSE OK ×2`；EncodedCommand 通道往返正常；manifest 远端
  读取与逐条哈希比对路径演练通过；guard 实测触发；临时 qa-parse 目录已清理并验证
  不存在。`Get-FileHash` 逐文件 SSH 调用改为一次远端读取 MANIFEST.sha256 + 本地比对。

## 4. 测试隔离与回归（continuation-review QA #5/#6，任务要求 4）

- `test-release-scripts.sh` 从 20 项扩到 **58 项，0 失败**，全部本机真实执行：
  - 打包测试改用**隔离 fixture 源码树**（$TMP 下 sed 替换 ROOT 的脚本副本 + 自建
    target/release），**完全不碰仓库 target/**——不可能替换/删除并发构建的真实可执行
    文件；新增"脚本 stub 二进制拒绝打包"（package-release.sh 现在对 `#!` 开头的
    目标二进制 fail-closed，stub 永不会被当作产品工件）；
  - 转录审计 12 个真实形状用例（好/内建工具混入/意外调用/无 init/空/零字节/
    多余服务器/未连接服务器/权限拒绝/hook 记录/模型错误/双预算超限）；
  - 启动器端到端 3 项（沙箱 print 运行通过+证据齐全；流氓调用失败；临时根 fail-closed）；
  - Windows 静态策略 10 项（含任务碰撞、SID ACL、EncodedCommand、manifest 自排除、
    token 不打印——断言精确到"不重定向到文件的 echo/printf $TOKEN"，不会误报写文件）。
- `tests/mcp_protocol.py`：新增 `--selftest`（隔离 fixture 模式：跨进程租约场景显式
  跳过并打印原因——产品专属行为；取消场景在 fixture 即时应答下只断言有界完成+
  响应性，不对 fixture 断言取消语义）。测试第 5 节内嵌一个**隔离假 host fixture**
  （有效 32×32 PNG、去重/冲突/暂停语义、-32700/-32601），harness 对其**端到端零失败
  通过**（qa-fix-1 未测项 #2 闭环，且 fixture 在 $TMP 不在仓库）。

## 5. 文档同步

- README：print 验收 flag、转录审计语义与非声明、证据位置、`--max-turns/--max-seconds`、
  已知源码副本/临时根 deny、Windows 碰撞/SID/复核语义、deploy 的 EncodedCommand 与
  manifest 自排除、mcp_protocol `--selftest`。
- `examples/acceptance/GUI-TEST-FIXTURE.md`：§3 运行过程更新为自动审计流程 +
  手工重跑命令；§4 证据清单改为 evidence 目录实物；§5 非声明增加"审计=策略合规
  非 GUI 正确性"、"deny 覆盖已知副本但不声称枚举人手拷贝"。

## 执行的命令与结果

| 命令 | 结果 |
|---|---|
| `scripts/test-release-scripts.sh` | **58 passed, 0 failed** |
| `bash -n scripts/*.sh` + `py_compile scripts/*.py tests/mcp_protocol.py` | OK |
| `scripts/run-blackbox-claude.sh --verify-only` | OK（22 argv 项） |
| `scripts/sandbox-probe.sh`（独立，含 4b/4c） | OK（源码树 + /private/tmp/rpa-core-shim + regex scratch 均拒绝） |
| 沙箱 print 端到端（$HOME 发布目录，假 claude 真实转录） | AUDIT OK 17 项，rc=0，证据齐全 |
| 沙箱 print 端到端（临时根发布目录） | fail-closed rc=126，运行不计数（预期） |
| 沙箱 print 端到端（流氓 Bash 调用转录） | 运行 FAIL（预期） |
| ssh acer-win 远端 PSParser 解析 ×2 | PARSE OK（未执行脚本、未启动 Host/GUI） |
| cargo 任何命令 | **未执行**（不重复 peer 的不可用测试；Rust 编译状态属 core/backend） |

## 未测项 / 明确非声明（诚实缺口）

1. **真实 claude + 真实 GUI 验收未运行**（协调者职责）。本 worker 的端到端证据是
   假 claude 进程发射的**真实形状**转录经过完整 sandbox+审计管线；真实 claude 2.1.251
   的 stream-json init 事件 schema（`tools`/`mcp_servers` 字段名）按官方 stream-json
   结构解析，首次真实运行时若字段名有偏差，审计会 **fail-closed**（init 缺字段即 FAIL），
   需按真实事件微调解析——这是有意选择：宁可误拒不可误放。
2. **审计通过 ≠ GUI 正确性**；视觉通道残余风险不变（GUI-TEST-FIXTURE §5）。
3. **PowerShell 脚本未在 Windows 执行**（仅远端语法解析 + 静态策略 + 本地 bash 侧
   演练）；任务碰撞拒绝、SID ACL、创建时间复核需在协调者首跑真实部署时验证。
4. Windows 部署的 scp/哈希全链路本次未对真实新二进制执行（无最新 Windows 构建产物；
   旧产物相对进行中的源码已过期）。
5. Rust 契约测试仍未运行（peer 范围；qa-fix-1 未测项 #1 维持）。
6. `/tmp/rpa-core-shim` 未删除（按指示不动未知 scratch；已 deny 并实测不可读）。
