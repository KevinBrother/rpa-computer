# Windows multidisplay generation token repair — SOURCE_FROZEN

日期：2026-09-30。Owner：Codex；仅实现与 Windows compile-check。Windows 实际执行由 CC 完成。

## 结论与当前证据等级

**修复源码已冻结；尚无修复后 Windows 测试 GREEN。** 已执行的最终验证仅为 display-topology Windows all-targets check、root Windows tests check、root Windows lib/bins check，均 exit0，无 warning。未执行 cargo test/no-run、测试 exe、应用 CLI、Host、SSH、GUI、截图/输入、Mac/Linux check/test；未启动 CC/agent，未 commit/push/worktree/reset。

旧 CC 真正结果仍保留：原29项 **26 pass / feedback 3 fail / native101**，全 lib 未跑，见 `.agents/reports/windows-multidisplay-green-cc-20260930.md` 和 `.agents/runs/windows-multidisplay-green-cc-20260930/evidence/filters-run4.output` 行78–91。不能用静态 review 的原“无阻断”结论覆盖该 RED。review 的两项低 severity 建议未实现、未扩范围。

本轮不改 Worker/McpService API、取消语义、feedback Rust/C# validators、renderer 或其他冻结库。协调通知的取消9项旧 exe GREEN 不等于 token 修复后回归；由 CC 在新产物上重验。

## RCA：生产编码错配，而非测试断言问题

1. 旧 `metadata::generation()` 使用 `format!("topology:{g:?}")`。Generation derive Debug 输出包含字段名、空格、花括号，例如 `topology:Generation { tracker: 1, revision: 1 }`。
2. root metadata 把此字符串写入真实 provider 的 `Geometry.version`；feedback authority 的 `surface()` 原样转入 `Surface.version`。
3. `FeedbackHandle::grant()` 在控制权授予之前执行 `surface.validate()`。Rust `validate_surface_version` 只允许非空、最多128 **字节**的 ASCII 字母数字与 `-_.:,`；C# `Token(..., true)` 同样只接受该 ASCII 集合，字符/字节长度一致。空格和花括号非法。
4. 两个 feedback 测试在 grant 处直接 `Protocol(InvalidField)`；第三个经 Runtime::open 把相同失败转成 `cancelled / desktop feedback refused the new session authority`。因此旧测试并未到达全部后续 capture forwarding、gap pointer、Stop 断言，不能宣称那些路径已验证。
5. 根因是 root 把 Debug 表示误当 wire codec。完整 authority+revision 的需求是对的，但“包含完整信息”不足以满足协议字符/长度约束。修复必须编码源头，不剥 Debug 标点、不只用 revision、不放宽 validators。

## 最小修改范围（4个源码/文档文件 + 本报告）

| 文件 | 修改 |
|---|---|
| `crates/display-topology/src/topology.rs` | 新增只读 `Generation::to_token(self) -> String`；新增5个纯 unit tests；原字段/相等/allocator/revision/exhaustion/update逻辑不变 |
| `src/backend/display/metadata.rs` | generation 投影调用正式 `to_token()`；注册独立 token_tests 模块 |
| `src/backend/display/metadata/token_tests.rs` | 新增2个真实 root Geometry→feedback authority/codec 回归；无自制替代 validator |
| `docs/windows-multidisplay.md` | 只更新 generation token 描述，移除 Debug 表示承诺，明确正式编码/生命周期 |

未修改原6RED和原3feedback测试；两者断言及文件字节保留。没有改 root/库 Cargo.toml 或 Cargo.lock，没有修改 desktop-feedback、windows-display、native-input、renderer、fixture、focus、runner、取消9项测试/文档或旧 SOL 报告/manifest/RED产物。旧 release 两bin/fixed exe 未删除、移动、重写；本轮无新release/test-exe产物。

### 正式 token 契约

```text
topology:tracker:<16位小写hex>:revision:<16位小写hex>
```

- 完整两个 u64，固定宽度、字段分隔明确，不使用 hash、截断、Debug 或字符串清洗。
- 对 `(tracker, revision)` 对是无损且无歧义的编码；相同 generation 稳定，不同tracker同revision不同，同tracker改revision也不同。
- **精确59 ASCII字节**：`topology:tracker:` 为17，tracker hex16，`:revision:` 为10，revision hex16；17+16+10+16=59，低于128。
- 不额外暴露可变字段或构造器；已有 revision accessor 和结构相等不变。
- tracker 是 process-local 分配，Host 重启后须刷新，token 不承诺跨进程全局唯一/永久身份。

### 长度静态纠正记录

本轮早期把前缀长度误算，曾将文档和新测试预期写成58；协调提醒后仅用字符串字面量做ASCII字节计数，确认59，已改正 topology.rs 文档、边界测试断言和 docs 三处，并在改正之后重新执行三项 Windows check。

这不是一次 Rust 测试运行；早期 cargo check 无法检出错误的运行期长度断言。早期 `check-topology.*`、`check-root-tests.*`、`check-root-product.*` 和 `*.sha256.pre-length-review` 草稿均保留，不作为最终冻结依据。报告下列 `check-final-*` / 正式manifest才是最终证据。没有覆盖原3FAIL证据。

## 新增回归源码（7项，未执行）

### 库：`topology::token_tests::`（5项）

1. `token_is_stable_for_unchanged_full_generation`：真实 tracker 重复相同事实 token 稳定。
2. `token_distinguishes_independent_trackers_at_same_revision`：两个真实 tracker、相同revision，完整 token 不同。
3. `token_changes_when_revision_changes`：真实 tracker 更新scale，authority不变/revision递增/token变化。
4. `token_is_bounded_ascii_at_u64_boundaries`：私有测试构造 `(0,1,MAX-1,MAX)` 组合，不修改全局 allocator；长度59、ASCII、<=128、允许字符，以及全零/全MAX精确编码。
5. `token_keeps_both_full_fields_without_concatenation_collisions`：36组包含易混淆数字和极值的字段对，唯一性与逐字段hex无损还原。实现的两个定宽字段本身保证无歧义，不以有限测试声称穷举证明整个u128空间。

### root：`backend::display::metadata::token_tests::`（2项）

1. `root_geometry_full_generation_survives_feedback_codec`：内存 FrameProvider 驱动**生产 DisplayProvider**，分别 Primary / exact ID / Desktop；取得真实 root Geometry 和真实库 capture mapping，核对 Geometry.version、describe topology_generation、observation topology_generation 一致。之后真实 FeedbackHandle::grant → Snapshot/Surface → 冻结 `encode_host` → `decode_host_line`，全消息严格相等。没有手工代造成功 Surface 或复制字符 validator。
2. `feedback_codec_still_rejects_legacy_debug_version`：先由生产 provider/Geometry 成功 grant，再将旧非法字面值送入 grant 和实际 encode/decode，均要求 `InvalidField`；防止通过放宽 validator“修复”。

这些新增测试只使用内存像素、纯库和反馈内存状态，不调用 GDI attach、OS枚举、屏幕捕获、真实输入、窗口、renderer、socket、进程或SSH。不得将此保证扩大为所有其他owner/root测试的副作用审计。

## 实际编译证据

新独占目录：`.agents/runs/windows-multidisplay-token-repair-sol-20260930/`。

```sh
source .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh
export CARGO_TARGET_DIR="$PWD/.agents/runs/windows-multidisplay-token-repair-sol-20260930/target"
cargo check --manifest-path crates/display-topology/Cargo.toml \
  --target x86_64-pc-windows-msvc --all-targets --locked --offline
cargo check --target x86_64-pc-windows-msvc --tests --locked --offline
cargo check --target x86_64-pc-windows-msvc --lib --bins --locked --offline
```

| 最终检查 | 原始日志/数字退出码文件 | 结果 |
|---|---|---|
| 库 all-targets | `check-final-topology.log` / `.exit` | exit0，0.48s，无warning |
| root tests | `check-final-root-tests.log` / `.exit` | exit0，1.37s，无warning |
| root lib/bins | `check-final-root-product.log` / `.exit` | exit0，0.13s，无warning |
| 本轮4文件 diff whitespace | `diff-check.log` / `.exit` | exit0 |

只格式化3个明确Rust文件，`rustfmt --edition 2021 --config skip_children=true`。没有链接/运行的新测试证据，不能据check断言3FAIL已转GREEN。

## CC 独立复验清单（以下命令未由实现者执行）

### 1. 新源码、新target、新exe哈希

先核对本报告新manifest；不要再以旧root源码manifest全部匹配为目标：旧manifest内仅 metadata.rs 与 docs 的hash理应改变，其余32项已核对未变。新库源码单独manifest14项，包括Cargo/lock/README及全部src/tests Rust文件。

```sh
source .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh
CC_RUN="$(mktemp -d "$PWD/.agents/runs/windows-multidisplay-token-cc.XXXXXX")"
export CARGO_TARGET_DIR="$CC_RUN/target"
cargo test --manifest-path crates/display-topology/Cargo.toml \
  --target x86_64-pc-windows-msvc --locked --offline --no-run --message-format=json \
  > "$CC_RUN/topology-artifacts.jsonl" 2> "$CC_RUN/topology-build.log"
# CC保存本条原始退出码；失败则保留并停止相应分支。
cargo test --target x86_64-pc-windows-msvc --locked --offline --no-run --lib --message-format=json \
  > "$CC_RUN/root-artifacts.jsonl" 2> "$CC_RUN/root-build.log"
# CC保存本条原始退出码。取消pure integration的独立构建按其owner清单进行。
```

从各次 JSON compiler-artifact/profile.test=true/executable 提取精确exe，不按glob取旧产物。库target包括 `rpa_display_topology` lib、`capture`、`mapping_drag`、`topology` integration；root lib是 `rpa_computer`。新Windows唯一目录按既有runner上传并双端SHA核对，保留stdout/stderr/numeric native exit/是否timeout/完整drain。不要把构建成功、空stderr或旧取消9项GREEN当本轮GREEN。

### 2. Windows 执行顺序及 filters

先复验原29项（断言未改）：

| root filter | 原项数 |
|---|---:|
| `runtime::runtime::tests::multidisplay_contract_red::` | 6 |
| `backend::display::tests::` | 6 |
| `runtime::runtime::tests::multidisplay_integration::` | 11 |
| `feedback::display_tests::` | 3（旧RED必须真实重验） |
| `mcp::bounded_output::tests::` | 2 |
| `runtime::execute::tests::topology_guard_cannot_dispatch_after_input_deadline` | 1 |

然后新增 root `backend::display::metadata::token_tests::`（2项）、库 lib `topology::token_tests::`（5项）；执行库全部默认纯tests（上述四exe，各无filter）；这些通过后执行 root **全lib默认suite**，`--test-threads=1 --nocapture`，不加 `--ignored`/`--include-ignored`。任何失败保留，不修改断言或重试掩盖，不预设全lib pass数量。

```powershell
& '<本次精确exe路径>' '<本节filter>' --test-threads=1 --nocapture
$code = $LASTEXITCODE
# 由既有runner按协议原样保存numeric exit。全suite省略filter参数。
exit $code
```

最后按取消测试 owner 的冻结清单重编译/执行9项pure integration，Worker API不用改。其他owner可能同时有阶段性RED，应分别归因记录，不把旧exe或独立renderer GREEN拼作本次root GREEN。

真实多屏/Stop/capture exclusion GUI仍须单独协调授权，不在本修复纯回归的证据范围；Mac/Linux仍暂停。

## 新冻结 manifest 与旧证据保全

以下文件均在本轮独占runs目录，**不是原StageB目录**：

- `source-freeze.sha256`：52条，含当前root原冻结集合+本修复、全display-topology源、未改的两Rust codec/validator文件和C# Protocol。SHA256：`1c1f515203afa66e6d709cafae5bce713b5e6f5f7232e5dcf08ce9343466d294`
- `topology-source.sha256`：14条库完整源码清单。SHA256：`3df869d2e67bf9ae4036db1a900d24bb45375aa140180545dc835000eee291c6`
- `compile-evidence.sha256`：最终三项check日志/exit。SHA256：`43705b7c82562b21aa0d5edc3fb3f47300764f1297634d1a338e07912c75c10f`
- 修复后 `crates/display-topology/src/topology.rs` SHA256：`8fb695bf6a1c3e346958055f66f9b11c2caf6b8797dc6091e3a7cd0a8741efca`
- 原6RED文件仍为：`79c30152b52c3f742cf974f81c5a2471e10539391dbb967ed796eee3c26cb3e1`
- 原3feedback文件仍为：`db4e8b8dac5f8f980a2c82df16604feb33da5db50bc8e7b6ad63ed18fccc1039`
- `unchanged-evidence.sha256`：14条已对本轮before.json核对不变的原测试、validators、Cargo/lock、旧SOL/CC测试报告、旧manifest及raw RED。SHA256：`6315162bbe2cc4462caef44f1b78d833a95eb74df1f97bc7f34be24254bc8fa2`。

共享树观察：修复期间其他owner在旧CC review报告末尾追加了 Correction，导致本轮读取前后该报告hash变化；本实现者没有写该文件、没有撤销其追加。`before.json` 与 `observed-changes.json` 保留该观察。不能声称CC review文件逐字未变；旧SOL报告、旧source manifest及raw RED确实保持原hash。

本报告自身hash另存 `report.sha256`，避免自引用。源码停止写入，等待CC新Windows证据；没有把新check或静态修复宣称为通过测试。
