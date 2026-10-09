# Windows stdout disconnect 生产契约 — CC 静态规格审查（2026-10-01）

范围：仅静态阅读生产代码与既有文档/fixture；未运行任何测试、未 SSH、未 GUI、未改任何源码/全局配置。不涉及 sol 正在编写的新测试。

## 1) stdout 读端断开在哪次 write 被发现；无写/执行中不可能即时检测

- 唯一的 stdout 写点是 `write_frame`：`src/mcp/stdio.rs:270-274`（`write_all` + `write_all(b"\n")` + `flush`）。主循环在发送每帧时检查结果：`src/mcp/stdio.rs:224-227` —— `write_frame(&mut out, &f).is_err()` 则 `break 'outer`，注释即 "stdout broken; nothing more we can do"。
- 因此检测**只能发生在下一次实际 write/flush 时**（管道读端关闭后 `write_all`/`flush` 返回错误）。
- **无写期间不可能检测**：主线程阻塞在 `service.handle_frame`（`stdio.rs:222`，进入 worker 的工具调用）期间没有任何 stdout 操作；不存在对 stdout 的读端监视/探活/回读机制（全文件仅上述 write 路径）。`event_rx.recv_timeout(50ms)`（`stdio.rs:210`）轮询的是入站事件与 shutdown_flag（`stdio.rs:178`），与 stdout 状态无关。
- **契约结论：即时检测 stdout 断开是未承诺、未实现的能力**，任何测试不得把它当作既有行为断言。

## 2) broken-output 退出后的 caller shutdown、cancel/release 与 numeric exit

- 写失败 `break 'outer` 后，`run_with_reader` 直接走到收尾：`worker.is_faulted()` 检查（`stdio.rs:261-266`）→ 返回 outcome。**stdout 写失败本身不改写 outcome**：它仍为 `StdioOutcome::Clean`（`stdio.rs:174` 初值），除非期间 worker 已 faulted。即 "transport 退出" 与 "业务 action 完成" 是两个维度：Clean 只表示循环结束且 worker 未 fault，不代表 in-flight action 成功或其响应被送达（响应恰恰永远送不到）。
- caller 处理链：`src/bin/computer-host.rs:405-417` 按 outcome 映射退出码（Clean→SUCCESS，Faulted→7，WorkerInitFailed→4）；随后 `computer-host.rs:443-451` 统一收尾：
  - `cancel.cancel()`（:443）——**注意：写失败 break 时 stdio 循环自身未 set cancel**，cancel 在这里（run 返回之后）才设置。~~这意味着从"检测到 stdout 断"到"cancel 生效"之间存在一个窗口：in-flight action（例如 5000ms key_hold）会继续自然执行，不会因 stdout 断开而提前取消~~ **【此句因果错误，见文末纠正 C1】**；
  - `worker.shutdown()`（:444）→ `src/mcp/worker.rs:522-538`：有界等待 native 线程；若 native call 未在界内返回则 `ShutdownStatus::Unknown` + worker faulted（`worker.rs:226-229, 488-489`）；feedback renderer stop 失败同样置 Unknown（`worker.rs:525-533`）；
  - `drop(desktop_lock)`（:445）释放 OS 写者锁；
  - 最终 numeric exit：`computer-host.rs:447-451` —— `shutdown_is_quarantined(shutdown)`（`worker.rs:160-162`）为真则 **exit 7**，否则用 transport 映射码（此场景即 0）。
- 契约区分：`StdioOutcome::Clean`（transport 层退出原因之一是 stdout 断）≠ 业务 `completed`；按 C1 时序，断管时的 in-flight action 必然自然结束（其 reply 丢失），caller cancel+shutdown 只作用于之后的清理与 native 线程收尾（若 native call 不在界内返回则 Unknown→exit 7）——两者从 outcome 不可区分，这是静态事实，不是缺陷断言。

## 3) stdin 仍打开时，生产 reader 线程的归宿

- reader 线程阻塞点：`FrameReader::run` 的阻塞 `read_bounded_line`（`src/mcp/jsonrpc.rs:235-241`，`n==0` 才 EOF）；线程体在 `stdio.rs:107-169` spawn，**JoinHandle 在块作用域内被丢弃，`run_with_reader` 从不 join 它**。
- 因此 `stdio::run` 返回后（含 stdout 断开路径），只要父进程保持 stdin 写端打开，该 reader 线程仍阻塞在 `io::stdin()` 读上；它不会感知后续在 :443 设置的 cancel（cancel flag 不中断阻塞 read）。
- 收尾方式是**进程退出**：computer-host `serve` 返回 `ExitCode`，进程结束时 OS 回收线程与 std 句柄。Windows 下进程退出会关闭继承的 stdin 读句柄，父进程的对应写端将得到 broken pipe/EOF。
- **风险边界（不扩大）**：这不是"已证明的线程/句柄泄漏"——没有任何证据表明资源在进程存活期内累积或逃逸到进程外。生产形态限定（纠正 C2）：默认无参 serve 运行 `stdio::run`（`computer-host.rs:405`）；`--listen` 分支运行 **`tcp::run`**（`computer-host.rs:428`），**不是 stdio**；remote 模式（`computer-host.rs:59`、模块注释 :76-79）由 supervisor 对每个认证连接 spawn 独立子进程、子进程内才是 stdio。因此"stdio loop 生命周期=进程生命周期"的陈述**只适用于默认 stdio 模式与 remote 每连接子进程**，不覆盖 loopback TCP。可陈述的边界仅是：**不存在 reader 线程被显式 join 的证据**（EOF fixture 报告 `windows-stdio-eof-sol-20260930.md` 亦已声明此限制），且若未来有人在长寿命进程内嵌 `run_with_reader`，被丢弃的阻塞线程会成为真实问题。当前产品形态下 `run_with_reader` 的生产调用点仅 `stdio::run` 一处。

## 4) 即将新增的真实匿名管道 stdout 断开测试：最少断言集

前置条件归属的明确区分：
- **stdin EOF（已在 windows_stdio_eof 覆盖）**：取消触发条件是 reader 读到 EOF → reader 侧 `cancel.cancel()`（`stdio.rs:166-167`）→ control slot Eof → `handle_eof`（`stdio.rs:185-188`）。
- **stdout 断开**：stdin 保持打开，取消**不来自 reader**；检测点是下一次 reply write 失败（`stdio.rs:224-227`）。因此测试的取消前置条件必须由"触发一次会写 reply 的交互"构成（例如关闭 stdout 读端后再发 initialize 这类必然回帧的请求，等待父端写写不进去后的超时/退出），而**不是**靠 stdin EOF、不是靠对 in-flight hold 的即时打断。

最少断言（均为契约内、不要求未规定能力）：
1. **idle 用例**：保持 stdin 打开，父端关闭 stdout 的**全部读句柄**，随后发送一个必然产生 reply 的请求（如 ping/initialize）。取消信号不会因父端"等待写不进去"而神奇产生——child 的退出依赖其自身下一次 `write_frame` 失败（`stdio.rs:224-227`）→ break → run 返回 → owner shutdown → 进程退出。断言 child 在有界时间内退出即可。
2. **active 用例**：父端在 hold 进行中关闭 stdout 读端。child 内部时序是确定的：`handle_frame` 同步阻塞至 hold 自然结束（`stdio.rs:222`，期间无任何 write）→ frames 返回 → 首次 write 失败 → 退出路径。**backend trace 必须精确断言 Press/Release 与正常 cleanup 计数、held 清空**，不得宽泛容许"任何终态"；断言中不得出现"stdout 断开提前取消 hold"。
3. **numeric exit 的边界区分（纠正 C3）**：EOF fixture 出错时的退出码 **70 是 fixture 自有 `main` 的**（`examples/windows_stdio_eof_fixture.rs:97-104`），**不是产品 Host 的 exit 7**；后者是 `computer-host.rs:405-417/447-451` 的产品入口，**fixture 并不执行它**。两者是不同边界，测试报告不得混用。产品 Host 入口的 stdout 断开行为仍是 pending 的运行结论。
4. **stdout.raw 完整性不做无条件要求**：父端中途关闭 stdout 读端后，child 此后写入的全部丢失，`stdout.raw` 可能只保留**前缀**（已写成功的帧）。最小用例若选择在"已完整收到上一帧之后、新 reply 产生之前"关闭读端，则可确定性地断言：收到的帧都是完整 JSON-RPC 行、断点之后无残行——而不是无条件要求全程完整。
5. `worker.shutdown()` 恰好执行一次、`post_shutdown` 调用 Dead（复用 EOF fixture 的 owner 收尾语义：stdio 返回后由 caller shutdown）。
6. 诊断与 JSON-RPC 分离：断点前收到的 stdout 内容只有完整 JSON-RPC 行，诊断在 stderr。

## 5) 有证据的生产缺陷

在静态证据范围内**未发现可断言的生产缺陷**。~~观察到两点非缺陷的契约缝隙，记录备查（均有行号，非猜测）：~~ **【第一条原表述因果错误，见纠正 C1】**
- ~~**检测-取消窗口**：stdout 写失败（`stdio.rs:226`）到 caller `cancel.cancel()`（`computer-host.rs:443`）之间，in-flight action 无取消信号、继续运行至自然结束；期间其响应不可达。~~（原引文保留于 C1）
- **reader 线程无 join**（§3，`stdio.rs:107-169` 丢弃 JoinHandle）：进程生命周期模型下无害，已如实记录边界；适用范围按纠正 C2 限定为 stdio 模式与 remote 子进程。

历史已验证的 stdin EOF 4 项（真实管道）不在本报告重复；TCP cancel07 与物理释放继续 deferred。无任何源码/测试/历史报告修改；未运行测试、未启动 Host、未操作桌面、未改防火墙。

## 纠正记录（2026-10-01，静态重核后追加）

### C1：原"检测-取消窗口"因果错误（影响原 §2、§5 第一条）

**原错误引文（保留）**："从'检测到 stdout 断'到'cancel 生效'之间存在一个窗口：in-flight action（例如 5000ms key_hold）会继续自然执行，不会因 stdout 断开而提前取消"；以及 §5 "stdout 写失败（stdio.rs:226）到 caller cancel.cancel()（computer-host.rs:443）之间，in-flight action 无取消信号、继续运行至自然结束"。

**错误所在**：这把"物理断管→下一次 write 才发现"的**等待**，错写成了"write 已检测失败→action 还在跑"的**事后窗口**。实际主循环是单线程同步 dispatch：`service.handle_frame(frame)`（`stdio.rs:222`）阻塞直至 action（含 hold 的完整时长）返回 frames，**之后**才进入 `for f in frames { write_frame… }`（`stdio.rs:224-228`）。因此：

- 物理断管发生在 hold 进行中时，child **毫无察觉**（无任何 write）——hold 按自身时长正常执行并正常 cleanup，这不是"检测后继续跑"，而是"检测前就在跑、结束才轮到 write"。
- 首次 write 失败（检测点）发生时，**action 已经结束**；随后 `break 'outer` → run 返回 → caller `cancel.cancel()` + `worker.shutdown()`（`computer-host.rs:443-444`）作用于一个**已完结**的 action，不存在 cancel 与 in-flight action 的竞态窗口。

**用不同事件区分的正确时序**：① 物理断管（父端关读端，child 无感知事件）；② write 检测失败（`stdio.rs:225`，仅发生在 handle_frame 返回后的 reply 写点）；③ run 返回（break 后 outcome 判定，`stdio.rs:261-267`）；④ caller cancel/shutdown（`computer-host.rs:443-444`）。②必然晚于 hold 自然结束；①与②之间可间隔整个 action 时长（这是**检测延迟**，不是取消窗口）。§2 对应句已划线标注；§5 该条已撤回。

### C2：§3 原把 loopback TCP 也算进"stdio 生命周期"——不严谨

原句"生产三种形态（stdio 默认、loopback TCP、remote 的每连接子进程）中 stdio loop 的生命周期都等于进程生命周期"不准确。`serve` 的 `--listen` 分支运行 **`tcp::run`**（`computer-host.rs:428`），不是 `stdio::run`；remote 模式由 supervisor 每连接 spawn 子进程、子进程内才是 stdio。已改为：该生命周期陈述仅适用于默认 stdio 模式与 remote 每连接子进程。

### C3：§4 原"numeric exit 0/7"混淆了 fixture 自有退出码与产品 Host 退出码

fixture 出错退出码 **70 是 `examples/windows_stdio_eof_fixture.rs:97-104` 自有 `main` 的**，与产品 Host 的 7（`computer-host.rs`）是不同入口、不同边界；fixture 不执行产品 Host 的 `main`，产品 Host 入口的 stdout 断开运行结论仍 pending。另外：父端中途关 stdout 读端后 child 后续输出丢失，`stdout.raw` 只能保证**断点前缀**为完整 JSON-RPC 行，不能无条件要求全程完整；最小用例应选在"已完整收帧后/新 reply 产生前"关闭，使该断言确定性成立。

### C4：§4 原表述"等待父端写写不进去后的超时/退出"含混

取消信号不会由父端的等待/写阻塞"产生"。idle 用例的正确模型：父端关闭 stdout **全部**读句柄 → 发送必回帧请求 → **保持 stdin 打开** → 依靠 child 自身下一次 `write_frame` 失败退出（同 C1 时序）。active 用例必须**精确**断言 Press/Release 与正常 cleanup、held 清空，不得宽泛容许任何终态。§4 已整体重写。

**边界声明：本报告为纯静态规格审查，不能证明任何 Windows 实际运行结果；上述所有行为结论均以源码行号为据，运行期表现（含管道错误的具体时刻与退出码分布）须由真实 Windows 执行另行验证。**
