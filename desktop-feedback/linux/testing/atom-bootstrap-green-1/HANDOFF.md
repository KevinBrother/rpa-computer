# Atom bootstrap minimal GREEN candidate — frozen handoff

2026-10-16。按用户明确授权修复已观测RED的产品契约，不等待历史GUI归因。
**未执行测试/SSH/GUI；无诊断sink、依赖/fixture/WM/compositor/Docker/Host/provider改动。**

## RED依据与唯一产品变更

已读 `.agents/runs/linux-parity-20261015/cc-ui-atom-red-1/probe.log`：Linux native-ui
实际4cases，两个指定正控assertion失败、两个control通过、rc101，不是compile/零case失败。
原73 RED author freeze及archive保持历史，不改原4/117任何断言。

唯一变更产品文件：`desktop-feedback/linux/src/native_ui/x11/atoms.rs`。
唯一行为变更：实际 `x::InternAtom` 的 `only_if_exists: true` → `false`；另更新两处说明。
13名称/字段顺序、先send全部再按序wait、已有atom ID、首reply错误传播和无重试语义不变。
connect仍使用同一生产seam，没有fixture预建atom或绕过真实初始化。
stdout错误code、rc、Ready位置、EOF优先级及capture_exclusion unsupported均不变。

## 身份与冻结

本目录 `source.sha256` 是完整repo-relative 76文件清单：原73（仅atoms.rs变化）+
本GREEN HANDOFF/native-check.log/integrity.log。完整archive `renderer-atom-bootstrap-green-1.tar.gz`
含这76文件；source.sha256单独交付，不放入archive，避免自引用。
最小overlay只需atoms.rs；完整清单/归档用于独立重建与审计。

原RED archive SHA256仍为
`9d40491778e8415ba12a74d744728398c11e11cf88f99be8115f86b153c55ebb`。
原66 archive SHA256仍为
`0796ba52582752bedbbc7446f669d6ddaa327c9270951c441d5b570eea57a7e6`。
完整比较见integrity.log：原73仅1产品路径变化；原4及117的17个tests/support路径byte-identical；
原RED archive逐项仍符合其历史manifest。旧freeze文档中的RED预期属于旧snapshot，不覆盖本GREEN。

## 本地compile-only

macOS Rust1.98.1；现有vendor与package-owned Cargo home/target；command-local DOCS_RS=1：

```sh
DOCS_RS=1 CARGO_HOME="$PWD/desktop-feedback/linux/target/cargo-home" \
CARGO_TARGET_DIR="$PWD/desktop-feedback/linux/target" \
cargo --config 'source.crates-io.replace-with="frozen-vendor"' \
  --config "source.frozen-vendor.directory=\"$PWD/.agents/runs/linux-parity-20261015/renderer-core-vendor-1\"" \
  check --manifest-path desktop-feedback/linux/Cargo.toml --locked --offline \
  --target x86_64-unknown-linux-gnu --features native-ui --all-targets
```

native-check.log rc0，包含新测试及实际binary源码。一条测试未使用intern_all的dead_code warning。
DOCS_RS跳过native探测/link flags：**不是实际Linux链接、Rust1.89/MSRV验证或测试GREEN**。
其他snapshot的回归/link证据不转移为这个新snapshot的接受结果。

## 独立CC命令（未由开发者执行）

使用现有approved builder/locked vendor，unset GUI环境，不设置DOCS_RS，不执行binary。
分别记录case数、断言及退出码；4cases不得接受零case结果。

```sh
cargo-1.89 test --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --features native-ui --test native_ui_atoms -- --nocapture --test-threads=1

cargo-1.89 test --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --no-default-features --no-fail-fast \
  --test native_ui_process_completion --test native_ui_startup_eof \
  --test native_ui_contract --test native_ui_policy --test native_ui_transport \
  --test codec_contract --test geometry --test ipc_input --test ipc_output --test model \
  --test native_ui_startup_races -- --nocapture --test-threads=1

cargo-1.89 build --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --features native-ui --bin rpa-desktop-feedback-linux
```

本snapshot的4+117与actualLinuxlink仍需独立CC验证，不能据compile-only宣称通过。
本轮不实施诊断sink，也不把历史首失败Xrequest/BadAtom归因作为后续修复门槛。
历史GUI实际记录仍是renderer_ui_failed；首失败Xrequest UNKNOWN。未来修后GUI成功只证明
修后行为，不能追溯补写历史错误。exclusion仍unsupported，Host admission关闭。
