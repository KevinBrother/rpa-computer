# Renderer atom bootstrap — production-connected RED freeze

2026-10-16。用户批准RED authoring；**only_if_exists=true缺陷保留，未修行为**。
没有测试执行、SSH、GUI、Docker/image变更、安装、compositor、Host/provider改动。
Capture exclusion保持unsupported，Host admission关闭。此快照不是live合格版本。

## 变更与生产调用链

相对已冻结原66，只有一个旧文件变更：
`desktop-feedback/linux/src/native_ui/x11/mod.rs`，将原atoms_struct声明移至新模块；
connect中原 `Atoms::intern_all(&conn).map_err(|_| UiFailure::Native)?` 调用保持原样。

新增产品文件：`desktop-feedback/linux/src/native_ui/x11/atoms.rs`。
实际链：X11Native::connect → Atoms::intern_all → intern_with → requests →
真实 `x::InternAtom` → `&xcb::Connection` send_request/wait_for_reply adapter。
测试使用相同intern_with和请求构造，只替换reply transport，不另造renderer判断算法。

保留原宏语义：13名称和字段次序、所有request的true值、先send全部再按序wait、原样返回NONE、
首reply错误传播且不重试、不返回partial Atoms。原facts/property调用和所有错误/EOF/Ready逻辑不改。
没有加入NONE防护或偷偷将必需名称改为false。现有atom ID保持，未新增依赖或改manifest/lock。

## 新4测试：预期2 FAIL +2 PASS，尚未执行

新文件 `desktop-feedback/linux/tests/native_ui_atoms.rs`：

1. `required_atom_requests_must_not_depend_on_a_wm_precreating_names`：捕获实际生产构造的
   InternAtom name/only_if_exists，独立golden核对13名称，必需名称要求false。现true应断言FAIL。
2. `absent_required_names_bootstrap_to_usable_atoms_without_fixture_prewarm`：空lookup registry
   输入经过实际生产bootstrap，要求13字段非NONE；当前应断言FAIL。oracle仅表达XML规定的
   absent+true→NONE / absent+false→创建规则；不模拟整台X服务器，也不声称真实server无预定义atom。
3. `existing_atom_ids_and_send_all_then_wait_order_are_preserved`：已有名称保留对应13 ID，
   先13send后13wait、映射字段准确，预期PASS。只是纯reply control，不向真实fixture预建atom。
4. `explicit_reply_error_is_propagated_without_partial_success_or_retry`：逐个13位置注入显式reply
   错误，必须传播相同error，send仍13次、wait止于首错，预期PASS。

无should_panic/ignore/assert失败stub。正控失败来自实际请求true与NONE值，不是故意Err。
Linux + native-ui cfg gate：独立执行必须报告实际4cases；零case（未启feature/错误平台）不是PASS。
这些测试不启动X连接、进程或线程，不测试真实窗口，也不验证本次GUI首错操作。

## 历史保持与身份

原archive仍为 `.agents/runs/linux-parity-20261015/renderer-process-eof-green-1.tar.gz`，
SHA256 `0796ba52582752bedbbc7446f669d6ddaa327c9270951c441d5b570eea57a7e6`，不重写。
本次逐项比原66：65完全相同，仅x11/mod.rs声明抽取不同；原16个tests/support路径全部byte-identical，
涵盖旧117断言。原117 PASS和旧实际link保留历史，不转移为新快照已运行证据。
原testing/process-eof-green.sha256及所有旧freeze不改，UI107 fixture未改。
新完整清单见本目录source.sha256；archive另交SHA，避免自引用哈希。

## 本地compile-only

macOS Rust1.98.1，使用现有vendor、package-owned Cargo home/target，无网络/依赖安装：

```sh
VENDOR="$PWD/.agents/runs/linux-parity-20261015/renderer-core-vendor-1"
export CARGO_HOME="$PWD/desktop-feedback/linux/target/cargo-home"
export CARGO_TARGET_DIR="$PWD/desktop-feedback/linux/target"
cargo --config 'source.crates-io.replace-with="frozen-vendor"' \
  --config "source.frozen-vendor.directory=\"$VENDOR\"" check \
  --manifest-path desktop-feedback/linux/Cargo.toml --locked --offline \
  --no-default-features --tests
DOCS_RS=1 cargo --config 'source.crates-io.replace-with="frozen-vendor"' \
  --config "source.frozen-vendor.directory=\"$VENDOR\"" check \
  --manifest-path desktop-feedback/linux/Cargo.toml --locked --offline \
  --target x86_64-unknown-linux-gnu --features native-ui --all-targets
```

本目录pure-check.log、native-check.log两项rc0。native检查包含新测试和实际binary源码；
新测试未使用真实connection入口有一条dead_code warning。DOCS_RS跳过native probes/link flags，
**不是实际Linux链接、Rust1.89/MSRV验证，更不是RED执行**。

## 独立CC命令（本轮未运行）

在现有锁定vendor/approved derived builder配置中，unset DISPLAY/XAUTHORITY/WAYLAND_DISPLAY，
不要设置DOCS_RS；各命令单独记录rc/实际case数，不运行ignored/live或整个未过滤测试集。

```sh
# 新4cases；预期2真实assertion FAIL +2 controls PASS、rc101。
cargo-1.89 test --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --features native-ui --test native_ui_atoms -- --nocapture --test-threads=1

# 原117（原107 + startup_races10），断言不变；与新RED分开统计。
cargo-1.89 test --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --no-default-features --no-fail-fast \
  --test native_ui_process_completion --test native_ui_startup_eof \
  --test native_ui_contract --test native_ui_policy --test native_ui_transport \
  --test codec_contract --test geometry --test ipc_input --test ipc_output --test model \
  --test native_ui_startup_races -- --nocapture --test-threads=1

# 实际link THIS snapshot，不执行binary。
cargo-1.89 build --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --features native-ui --bin rpa-desktop-feedback-linux
```

诊断仅设计于DIAGNOSTIC-DESIGN.md；没有新增日志行为。本次真实GUI依旧只有renderer_ui_failed，
不能把新RED预期当成实测BadAtom。后续行为GREEN和任何诊断/live执行都需要协调者另行授权。

打包口径：新完整archive包含原66路径（其中1处接线变更）+2新Rust+本目录5个文档/日志，
共73文件。`source.sha256`为独立sibling清单（73 entries），不装入archive，避免自引用；
协调者复制archive时需同时携带此清单。新Rust/接线共3路径可单独overlay原66。
