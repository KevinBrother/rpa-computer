# macOS filtered capture — Codex / sol 独立库冻结交接

日期：2026-09-30。工作目录：`/Volumes/doc/workspace/datagrand/rpa/rpa-computer`。

## 当前阶段与最新优先级

- 已完整读取 `.agents/tasks/macos-filtered-capture-sol-20260930.md` 并完成本批独立实现。
- **源码及必要编译检查完成；没有执行任何测试、capture、TCC preflight或GUI验证。**
- 最新用户优先级：先测 Windows；Mac/Linux 测试与 Host Mac 整合后排。此库冻结交付，不要求 Windows 等本库测试/整合。
- 写集仅新 `crates/macos-capture/**` 与本报告。`desktop-feedback/**` 未再修改；root Cargo/src、Host、native-input、feedback库、旧fixture没有被本 agent 修改。
- 无其它 agent/Claude CLI启动、GUI/截图/SSH、TCC更改、worktree、commit/push或脏树恢复。

## 已完成代码阶段

### 独立 API 与信任边界

- package：`rpa-macos-capture`；Rust导入 `rpa_macos_capture`；独立 `[workspace]`、Cargo.lock，无root Cargo编辑。
- `CaptureClient::new() -> Self`：纯Rust构造，不调用native/TCC/capture，不spawn线程。
- `capture(&mut self, CaptureRequest) -> Result<CapturedImage>`：request 为display_id/excluded_process_id/width/height/timeout，结果为PNG Vec<u8>及实际width/height。
- `is_supported()`：OS version检查；`is_busy()`：未完成flight门禁。
- 同步API不得在main/UI thread调用，直接返回结构化InvalidRequest；Host后续应缓存client放在capture lane。
- PID是Host提供的trusted owned renderer child PID，不从Agent参数获取。库只按exact PID选择唯一SCRunningApplication；不匹配名称、bundle ID、标题，不导出NSError描述。
- ownership/liveness/PID复用由持有真实child的Host负责；库没有伪称验证进程父子关系或Agent授权。

### 原生过滤与PNG

- Foundation OS>=14检查；`CGPreflightScreenCaptureAccess()`拒绝时不进入discovery，不调用授权request。
- `SCShareableContent`发现真实display/application，选择exact displayID和exact PID；缺失/歧义fail closed，不能空过滤/普通截图fallback。
- `SCContentFilter.initWithDisplay_excludingApplications_exceptingWindows`：排除唯一目标app，exceptingWindows为空，不重新包含renderer。
- `SCScreenshotManager.captureImageWithFilter_configuration_completionHandler`：本机SDK与Apple primary文档都标为macOS14.0起；不调用15.2/26专用API。
- ImageIO直接编码真实CGImage，保留native色彩/stride/方向路径；不手工复制/翻转BGRA，不绘制像素，不hide-then-capture。
- 实际CGImage尺寸及PNG IHDR必须一致，不把请求width/height伪装成实际尺寸；actual尺寸也执行预算校验。

### Deadline / late callback / flight

- 一个absolute deadline覆盖整个pipeline，不按阶段重置；请求timeout>0且<=30秒。
- 每client一个flight，超时后保持Busy，直到callback complete；callback永不返回时client不再派生新调用。
- discovery/image各有claim gate；重复callback不启动额外截图/编码，first completion不可被覆盖。
- RcBlock只捕获heap Arc、Retained资源和Copy request；不借用client/栈channel/栈变量。client drop后迟到回调只访问其自身heap state。
- filter/config/content/initializer数组pin到screenshot completion block，configuration发出后不再变更；原始CGImage只在回调有效期内同步编码。
- 超时前已dispatch的OS调用无强制cancel；调用线程在deadline停止等待，但flight仍隔离；不得以重建client的方式无限重试。
- 没有每次spawn专属永久线程。使用系统completion+Rust Mutex/Condvar；Rust panic fence转静态错误，未伪称通用ObjC异常catch。

### 预算与诊断

- dimension<=16384，total pixels<=33554432，pid为1..i32::MAX；数学使用u64/checked_mul。
- shareable显示器<=256、应用<=16384；超限fail closed。
- actual raster<=256MiB+1MiB；PNG<=128MiB+1MiB。
- CaptureError字段：kind/stage/native_domain/native_code；static code：unsupported/permission/no_display/excluded_process_missing/invalid_request/busy/timeout/capture_failed/encoding_failed/invalid_image。
- SDK核实SCError：-3801 UserDeclined、-3803 MissingEntitlements、-3814 NoDisplayList、-3815 NoCaptureSource。初稿测试里的-3810 NoDisplay推测已在读SDK后纠正，生产/测试均使用真实分类；没有调用错误API去验证。
- CapturedImage Debug只有尺寸及PNG字节数，不dump实际屏幕内容。

## 测试代码状态（未执行）

`src/tests.rs` 共17个纯测试声明，使用生产共用的Deadline/Flight/FlightSlot/selection/PNG guard/classification逻辑，不是平行oracle：

- 尺寸/像素/PID/timeout预算、共有absolute deadline。
- exact display/PID、prefix/邻近PID不匹配、缺失/歧义/过大列表fail closed。
- timeout后flight维持Busy、迟到回调/client drop、callback永久缺失1000次重试仍Busy。
- discovery/image重复callback、first result不覆盖、旧callback不触及新flight。
- 早完成但迟交付不能报告成功；failed stage可以完成并释放gate。
- PNG header与actual尺寸核对、budget拒绝、static error隐私与真实native分类。
- new client无flight的纯状态检查。

没有真实capture测试、ignored GUI test或截图runner；没有把compile check当测试通过或过滤效果证据。

## 精确开发命令与结果

工作目录均为本项目。**只有编译/工具链/源码读取，无测试运行。**

- `rustc --version`：exit0，`rustc 1.98.1 (48a229cea 2026-09-01)`。
- `cargo --version`：exit0，`cargo 1.98.1 (797e8a9bc 2026-08-05)`。
- `xcrun --show-sdk-version`：exit0，`26.5`。
- `cargo fetch --manifest-path crates/macos-capture/Cargo.toml`：第一次manifest缺目标，exit101；创建src/lib.rs后同命令exit0，生成本库Cargo.lock（没有改root lock）。
- `cargo check --manifest-path crates/macos-capture/Cargo.toml --all-targets`：初次exit101，NSString多AsRef候选歧义，修正为明确&NSString后exit0。
- 裁剪default features后同check初次exit101，NSArray.iter依赖NSEnumerator；核查维护者源码并显式加该feature后exit0。
- `cargo fmt --manifest-path crates/macos-capture/Cargo.toml`：exit0，仅本库格式化。
- 最终严格编译：
  ```sh
  RUSTFLAGS='-D warnings' cargo check --manifest-path crates/macos-capture/Cargo.toml --all-targets --locked
  ```
  exit0，末行：
  ```text
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.29s
  ```
  编译了production和17个测试的源码，**没有执行测试体，没有运行capture，没有native GUI/权限调用**。

## 依赖锁定

直接目标macOS依赖：objc2 **0.6.4**，block2 **0.6.2**，objc2-foundation / screen-capture-kit / core-graphics / core-foundation / image-io 全部 **0.3.2**。
具体feature在本库Cargo.toml，default features已裁剪，未引入输入、LLM、网络、renderer或测试runner依赖。
Cargo.lock另锁定objc2-encode4.1.0、libc0.2.189、bitflags2.13.2、dispatch2 0.3.1、objc2-io-surface0.3.2；lock存在不等于本次所有可选module都被启用。

API availability/SDK声明/官方primary链接及所有权审阅：`crates/macos-capture/docs-api.md`。

## 明确未完成 / 后排事项

1. **未接线root Host**，当前产品Mac截图仍未被本库替换；这是新库交付，不是“产品截图排除已完成”。另一个Host owner后续负责接入，当前后排。
2. `desktop-feedback` 仍冻结，其ready.capture_exclusion=unsupported是renderer本地API能力事实；本库不能篡改ready或把is_supported当实际过滤验收证据。
3. macOS最低功能API14；旧版本有运行时Unsupported路径，但maintainer binding有framework链接。若Host要分发到尚未含ScreenCaptureKit的旧系统，packaging需单独处理optional/weak linking，不能以selector guard保证整个旧安装包可启动。本次没有扩展旧OS部署工作。
4. 未实测CLI/AppKit renderer的exact PID是否出现在SCK applications；找不到就明确失败，不能按名称猜/忽略过滤。
5. 未实测真实排除、多屏/混合DPI、颜色/方向/actual尺寸、权限变更、系统回调时机或真实deadline行为。
6. 当前没有Mac/Linux cargo test、clippy、真实capture、GUI/TCC/SSH。Windows优先，由协调者CC+GLM独立安排，不被本库阻塞。

## 交接命令（记录，不要求现在执行）

开发compile可复用：

```sh
cargo check --manifest-path crates/macos-capture/Cargo.toml --all-targets --locked
```

**Mac/Linux测试现在后排**；待协调者恢复排期后，CC+GLM才执行：

```sh
cargo test --manifest-path crates/macos-capture/Cargo.toml --lib --locked
```

后续真实验证另行授权、由CC安排：Host-owned renderer人眼可见，同时原始filtered PNG及最终model图片不含bar/Stop/ring；其它软件仍可见；missing PID/display/permission失效时不走unfiltered fallback；图像尺寸/色彩/方向；Timeout/Busy与Host控制策略的接线。

## Changed files（全部新文件，独占写集）

```text
crates/macos-capture/.gitignore
crates/macos-capture/Cargo.toml
crates/macos-capture/Cargo.lock
crates/macos-capture/README.md
crates/macos-capture/docs-api.md
crates/macos-capture/src/lib.rs
crates/macos-capture/src/error.rs
crates/macos-capture/src/request.rs
crates/macos-capture/src/selection.rs
crates/macos-capture/src/flight.rs
crates/macos-capture/src/native/mod.rs
crates/macos-capture/src/native/encode.rs
crates/macos-capture/src/tests.rs
.agents/reports/macos-filtered-capture-sol-20260930.md
```

Ignored编译产物仅本库 `crates/macos-capture/target/**`。所有新文本文件<1000行，没有回写冻结的renderer或root代码。
