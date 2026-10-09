# Windows multiclick — read-only evidence rescue & corrections (CC, 2026-10-08)

角色：只读取证/复核（无桌面操作、无测试执行、无实现/测试/helper 修改、无删除/移动文件、无权限改动）。所有原始大文件存放于 `/private/tmp/windows-multiclick-diagnosis-20261008/`（Data 卷，实测 42Gi 可用），仓库内仅本报告。本报告不重写上一份报告（`windows-current-multiclick-cc-20261008.md`），以下为独立勘误与补充。

## 0. 交付物（raw 优先）

- **host log 位置清单（首个交付）**：`/private/tmp/windows-multiclick-diagnosis-20261008/host-log-location.txt`
- raw 文件（全部 SHA256 留痕、逐一校验）：
  | 文件 | 大小 | SHA256 (前8…后4) | 说明 |
  |---|---|---|---|
  | `host-log-current-20261008.log` | 4,812 B | `89A2A3AE…B574` | 当前 host.log，scp 后与远端一致 |
  | `host-log-archive-multiclick-20261008.log` | 1,806 B | `4C6A2396…4A3A3D` | 换 bin 前归档 host.log（实际位置见 §1） |
  | `oracle-evidence-remote-authoritative.jsonl` | 42,578 B | `2FE4A011…1ADD` | 远端现行 oracle（权威版） |
  | `outer-cc-session-recovered-6d20c43d.jsonl` | 2,495,278 B | `44EC7C09…C1D4` | 恢复的外层 CC 会话（421 行） |
  | （保留原样）`evidence-cc-transcript.jsonl` 30,849 B `7070F67E…5B01`、空 `evidence-cc-stderr.log` | | | 被删会话遗留文件，未触碰 |
- 内层 transcript / run-summary / run-record 本地副本经远端 SHA 比对为**逐字节一致**（transcript `C2AA99FC…6951`、run-summary `0588A793…8A53`、run-record `7E67317E…616E`、stderr 0B），未重复拉取。

## 1. host.log 权威证据（当前主机，GUI 已停）

- 当前 `host.log`（4,812 B）记录本轮 8 个远端 client 会话：
  - 3 个早期干净会话（:65182/:59108/:52735，`stopped` 正常退出）；
  - **4 个失败会话（:50281/:63706/:54586/:63588）**，均 `remote transport failed: tls writer accepted 0 bytes` + `remote pump ended with error`；
  - 1 个稳定会话（:55604），正常结束。
- 勘误①：上一报告 §3 称"开局 4 次 open 中 **3** 次 observe 失败"，且其列出的 session id 却有 4 个（自相矛盾）。host 侧与内层 transcript 双向证实为 **4 次失败**（见 §3）。
- 勘误②：§10 称 host.log.archive 位于根目录附近；实际路径为 `C:\computer-cc-preflight-20260930-191500\bin-backup-multiclick-20261008-111947\host.log.archive`（SHA 与报告所引一致 `4C6A2396…4A3A3D`）。
- host 侧无崩溃/无鉴权失败记录；失败均为主机向 client 写出时 0 字节接受（对端断开形态），与内层观察到的 "Connection closed" 吻合。

## 2. 外层 CC transcript 恢复（报告存在捏造路径，证实）

- **已恢复**：`~/.claude/projects/-Volumes-doc-workspace-datagrand-rpa-rpa-computer/6d20c43d-680e-4c15-9586-1a2a6624d490.jsonl`（mtime 2026-10-08 11:49，2,495,278 B，421 行），首个真实 user 任务文本逐字匹配 `'# GO: current Windows product + first multiclick GUI suite, 2026-10-08…'`。已按字节复制到 `outer-cc-session-recovered-6d20c43d.jsonl`（SHA 见 §0）。
- **格式声明**：这是 Claude CLI 持久化会话 JSONL（含 system/assistant/user type 行），**不是**丢失的原始 stream-json stdout，缺失的 init 前原始流/管道输出不可静默重构；本恢复件仅为 CLI 侧会话记录。
- 勘误③：上一报告 §0 所称外层 transcript 路径 `.agents/runs/windows-current-multiclick-cc-20261008-outer/` **不存在**（实测 `No such file or directory`），系捏造路径；真实持久化位置如上。

## 3. 五项主张的原始数据核对（不采信报告转述）

### 3.1 初始 observe 失败次数（勘误① 成立）
内层 transcript（SHA 同远端，下同）逐条：`computer_open` 共 **5** 次。前 4 次（session-31688/-27588/-2036/-31300）各产生**一次** observe `Connection closed`（is_error=true）+ 随后一次 observe `session_not_found` —— 即 **4 组** "Connection closed → session_not_found"。第 5 次 open 带 `max_width:1024, max_height:768` → `session-29628-1` observe 成功并稳定至结束。**稳定会话出现前无任何输入**：第一个 `computer_step` 位于第 16 个工具调用（成功 observe 之后）。任务规定的"初始可见性失败即停"未被执行——属协议偏离记录（与上一报告一致），但"未在失败会话派发输入"属实。audit 17 项通过不改变此定性。

### 3.2 multiclick-08（勘误④ 成立：报告归因错误）
- 内层输入序列（request_id 逐字）：`case8-left-1`（左击 512,288）→ `case8-right`（**button:"right"** 512,288）→ `case8-check`。**第三个左击从未派发**。
- 权威 oracle（42,578 B 版）该例 input_event：event 1/2 = left down/up，event 3/4 = **right down/up（真实右键按下/抬起，native 应用已收到）**。
- fixture 判定 `matched:false, reason:"left/right/left resets", downs=2` —— downs=2 是因为模型只派发了 2 次点击（左+右），**不是**报告所称"右击未被应用计数"。应用侧行为正常，失败归因于内层未完成 左→右→左 序列。

### 3.3 multiclick-04 / 05（勘误⑤ 部分成立）
- **04**：oracle gesture_check `selection='alpha '`（**尾部 U+0020 存在**），reason "native word selection"。上一报告称 "word selection absent" —— 错误；选择存在但带尾随空格（因此未匹配期望）。
- **05**：`selection=''`（空）属实；fixture UI 指令文本在 "On Windows native EDIT, report the" 处被截断属实（已目检截图 call029；oracle `instruction` 字段仅为第一句，UI 全文被裁切）。两例均无产品失败归因证据，维持"未知/不归因"。

### 3.4 SUITE COMPLETE 与 analyzer suite_complete=false（语义分歧，不归一）
- fixture UI "SUITE COMPLETE — 10 cases visited (not a pass count)" = 遍历完成；oracle `suite_complete` 事件 `visited:10`（03:27:59Z）。
- analyzer（scripts/analyze-gesture-gui.py:305-318）`suite_complete` 要求：恰 1 个 suite_complete 事件 + 10 trials + **全部 first 结果非 unknown**。本轮 3 例 unknown → false，`status=partial`（实测 report-analyzer.json：first_matched=7、first_input_successes=6、gui_verified=false、model glm-5.3-flash verified）。两者词汇不同、各自成立，**不得把 analyzer 结果归一为通过**，也不得把 UI 遍历完成当作通过率。

### 3.5 events_completed 序号跳跃（勘误⑥：非缺陷，且报告引用错误）
- 产品语义（src/runtime/execute.rs:31-36,101-105,224）：`events_completed` 为**计划内 1-based 索引**，`PlanEvent::Sleep` 占索引但不计入 `events_total`（total 排除 sleep）也不产生完成条目。跳号（case2 的 3；case3 的 3、6）即**点击间调度的 multiclick 间隔延时**，属设计行为，非丢失输入。
- 勘误：上一报告 §4 注 "如 [0,1,2,4,5]、events_total 7 实成 5" 系错引——`[0,1,2,4,5]` 是 case2（total 5，全成）；case3 实为 `[0,1,2,4,5,7,8]`（**7 条全成**，total 7）。应用侧判定不受影响这一结论仍成立。

## 4. 其余取证结果

- **oracle 版本差异（新发现）**：远端现行 oracle（42,578 B）比本地/verifier 快照（42,292 B，`EB30833E…9E3D`）多 1 行 `session_close`（2026-10-08T03:42:55Z，即 11:42:55 本地）——fixture 任务在清理停止时追加的收尾事件，**晚于** suite_complete（03:27:59Z）。verifier/analyzer 所用快照包含全部 10 例判定事件，其结论不受影响；但**引用 SHA 时必须区分**：`EB30833E…`=快照，`2FE4A011…`=现行权威版。oracle 文件带 UTF-8 BOM。
- **stderr.log**：远端与本地均 0 字节（SHA `E3B0C442…` 空文件）。
- **launcher 运行输出原件**：远端根目录仅 `host.log`+`host.token`，C:\Temp 仅 verifier 目录，run 目录内无 launcher 日志 → **远端 launcher 输出原件不可得（unknown）**；仓库 run 目录内 10 份小日志为上一会话从 transcript 重建件（已由其自行标注），本轮未改。
- verifier 目录远端仍在：`C:\Temp\verifier-multiclick-20261008-20261008-113724\`（脚本复哈希与报告一致：analyze `F86CF62C…0C04`、audit `D4C22DCD…87E3`）。
- fixture run record：`run-6361811f55294ba9a872c203a4c338f2`，pid 20880，started 03:21:08Z；inner run dir `5ec5ca5c-caa6-4700-8ad1-09719ec44ee0`，exit 0，MaxTurns 90/MaxSeconds 900。均与上一报告相符。

## 5. 边界

- 全程只读（ssh 仅取元数据/日志/scp 取文件）；未执行任何 GUI/测试；未删/未移任何文件；未改上一报告；未触碰凭证/host.token 内容。
- 本报告仅覆盖 multiclick 子树 + 已知 host.log；未检查无关会话与秘密。
- 协议偏离（§3.1）与 host 启动期 4 次断连（§1）留 host/流程方跟进；模型保持 glm-5.3-flash（transcript 实测 123 条 assistant message.model 全一致）。

## 协调者核对与补充收窄

已对取回的host.log、恢复会话文件及清理后oracle重算SHA，与上表一致。新oracle逐字节保留旧oracle作为前缀，仅追加一条session_close，旧快照不改。

- Rustls写入返回0的根因仍由实现作者定位；本日志本身不证明“对端断开”，暂删去该因果推断，只能证实Host将0接受判为传输错误。
- events_completed实际输出及execute.rs的push(index)为0-based（Sleep占计划索引但不计input total），现有源注释写1-based与实现不符；本报告§3.5引用注释不能改变实际数据。跳过Sleep的索引不是丢失输入，case2是5/5、case3是7/7。
- evidence-cc-transcript.jsonl是本次99369取证作业正在写入的外层流，不是被删旧会话遗留；中途对活跃文件算的大小/hash不是冻结终态，§0对应值不当作最终凭证。旧外层恢复件为明确命名的outer-cc-session-recovered-6d20c43d.jsonl。
