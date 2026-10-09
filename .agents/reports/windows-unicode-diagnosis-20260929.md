# Windows Unicode 输入异常定位（2026-09-29）

## 结论与状态

本轮完成第一阶段定位，**未修改产品输入逻辑，未宣称修复或最终根因已确定**。

- 无 LLM 决策、固定 MCP 请求即可复现；自有记事本的 UIA 文档文本与失败截图一致。不是仅靠截图/模型误读解释的问题。
- 独立 C# PInvoke `SendInput` 绕过 MCP、Runtime、Rust 和 Enigo，也复现字符丢失/重复末尾字符。它们不是产生此类现象的必要条件；不能反推所有历史失败都一定同一原因。
- 原生失败时：helper 检查每次注入前 foreground PID=自有记事本；完整事件数组与预期 UTF-16 的 down/up 序列一致；SendInput 返回完整事件数；读回文档不一致。因此“成功发送事件”不能用作“目标应用正确输入文本”的充分证据。
- **尚未分清 Windows 输入子系统、IME/文本服务、目标应用内部处理或其交互状态。** 不能直接归罪 Windows/Notepad，也不能声称产品侧已经完全无问题。
- 逐字符 10ms 在较长混合文本中 2/2 漏字。不得把增加固定延时当作已验证可靠修复；禁止以自动重试掩盖失败。

## 实验条件与安全边界

- acer-win，Windows 交互桌面 Session 1，Notepad `11.2607.14.0`。
- 启动前确认无 Notepad；本轮 launcher PID25568，editor PID21160，通过路径、创建时间和 session 确认所有权。
- UIA 仅读取该自有文档作为诊断 oracle；未使用 SetValue、剪贴板或文件写入绕过输入。
- MCP 实验路径：本机 Client → 网络 TLS Host → Runtime → Enigo → Windows。
- 原生实验：上传 helper → Interactive Scheduled Task → C# PInvoke SendInput → 自有 Notepad → UIA 文档读取。
- SSH 只用于文件传输、任务编排、身份核验与日志；没有从 SSH 服务会话直接操作 GUI。
- GUI 输入串行；中性背景仅启动时置前自有窗口，无循环输入或键盘钩子。
- 本轮诊断不是 agent 黑盒验收，未补齐每平台每案例10次/≥8次成功门禁，也未解决原验收 OS 文件隔离证据缺口。

## 结果：35 次有文本真值的输入实验

| 组别 | 次数 | 文本匹配 | 文本不匹配 | 说明 |
|---|---:|---:|---:|---|
| 固定 MCP（15 batch + 2 chars） | 17 | 15 | 2 | 多行按 CR/LF 归一化；同文本后续也能成功 |
| 原生短文本，batch/char0/char10 各2次 | 6 | 6 | 0 | 仅这6次成功不能排除底层问题 |
| 原生长混合单行，三模式各2次 | 6 | 1 | 5 | batch 0/2，char0 1/2，char10 0/2 |
| 原生字符替换/位置对照 | 6 | 4 | 2 | 移除全部emoji的batch仍失败 |

这些是探索性、有顺序和状态依赖的样本，不是随机试验，不代表稳定成功率。

### 样例 A：MCP，无换行、无代理对

请求：`Unicode 测试 ✅`

实际：`Unicode ✅✅✅✅`

证据：`.agents/runs/unicode-case-03-mixed-batch/`（请求、截图、UIA与 comparison.json）。相同文本逐字符发送成功；相同文本随后 batch 6次成功（case09 + case13×5）。因此不是固定字符串必错。

### 样例 B：独立原生批量，同类损坏

请求：`你好，世界 🌍 | Unicode 测试 ✅ | 本文档共 3 段（含本段）`

原生 batch 第一次读回：`你好，世界 🌍 ）`

第二次读回：`你好，世界 🌍 ` 后接30个 `）`。

- 38个 Unicode scalar，39个 UTF-16 code units，共78个文本输入事件。
- 逐一核对 wScan、wVk、dwFlags 与请求相符，surrogate down/up 成对；SendInput 接收78/78。
- 读回前后 foreground PID21160；helper在每次SendInput调用前再次核验前台PID。该采样不等于全程焦点/控件事件监控。
- char10 两次均漏掉第一个 `|`，不是完全正确。

证据：`.agents/runs/unicode-direct-native-long-20260929/`。

### 样例 C：排除“必须包含emoji/换行”

请求：`你好，世界 X | Unicode 测试 Y | 本文档共 3 段（含本段）`

独立原生 batch 读回：`你好，世界 code ` 后接重复 `）`。

全部是 BMP 字符，无换行，也无emoji。此失败证明 surrogate 与换行特殊分支都不是产生这种损坏的必要条件。对应 char10 本次成功，但不能抵消其他 char10 失败。

证据：`.agents/runs/unicode-native-ablation-20260929/noemoji-batch.json`。

## 源码事实：另有应单独处理的依赖问题

路径：`src/backend/dispatch_core.rs` → `src/backend/dispatch.rs:94-99` → enigo0.3.0 Windows `text()`。

1. Runtime 每64个 scalar 分块（`src/runtime/plan.rs:36-53,211-215`）；12字符及34字符失败请求均未跨块，不能归因于跨块边界。
2. enigo `win_impl.rs:466-497`：每个UTF-16单元 keydown 使用该单元，但 keyup 使用 `result[0]`。对于非BMP字符，低位代理的keyup值不匹配。BMP 样例不受此差异影响，原生正确构造也仍失败。
3. enigo `win_impl.rs:270-292`：`\n`/`\t` 生成 Return/Tab后仍继续 queue_char，构造出两套事件；`\r` 空分支后同样继续 queue_char，**不是被丢弃**。这些事件生成事实需独立回归，不能拿来解释无换行BMP失败。
4. 整批注入本身不是单凭源码即可认定的缺陷；其可靠性需在目标环境验证。

依赖源码：`~/.cargo/registry/src/rsproxy.cn-e3de039b2554c837/enigo-0.3.0/src/win/win_impl.rs`。
SHA256：`03f97b9d26f4c51f2c9f0d9df72dbb7b38f4fce133e2f5828860b707157ef56f`。

## 已缩小范围，但尚缺的证据

- 发送前后的目标线程键盘布局、IME/TSF状态、修饰键状态未完整采集。
- 原生helper记录的是调用参数与返回数，不是Notepad真正消费到的 WM_CHAR/文本服务事件。
- 无同一组原生事件、相同条件下的标准WinForms Edit与Notepad配对对照；不能只用历史夹具成功推导Notepad独有。
- MCP慢速逐字符包含网络往返和截图等待，不等价于原生无延时拆分。
- 原生probe等待500ms后读一次；MCP失败还有后续截图/UIA证据。若下一步研究合成/异步处理，应增加有界多时点只读回读，区别稳定损坏与暂态。

建议下一阶段按顺序：
1. 新建自有标准WinForms编辑控件，与现代Notepad做相同原生输入对照；固定进程/控件焦点、键盘布局和输入法状态，采集作用域受控的事件与多时点文本。
2. 先区分应用特性、输入法/文本服务和注入节奏；不要默认模型差、宿主传输错误或所有Windows都存在此问题。
3. 为依赖的surrogate/key-up及换行构造单独写失败回归，再设计输入适配。任何慢速模式须验证取消粒度和截止时间，不能用“无脑sleep”破坏执行契约。
4. 产品继续区分 dispatched 与任务成功；agent根据观察核验结果，不能把SendInput返回数作为文本成功oracle。

## 证据索引与清理

- MCP汇总：`.agents/runs/unicode-comparisons-20260929.json`。
- 原生汇总：`.agents/runs/unicode-native-comparisons-20260929.json`，18条均逐事件验证；11条匹配、7条不匹配。
- helper由本机Claude CLI（配置GLM）编写，协调者执行与审查；没有产品Rust输入逻辑变更。
- 独立审查：`.agents/reviews/unicode-diagnosis-20260929.md`；已纳入18次原生证据并经协调者复核纠正计数与过度归因；不能用初审“原生未复现”覆盖长文本证据。
- 本轮自有文档通过GUI关闭tab并选择不保存；关闭动画截图后，独立进程核验确认editor与launcher均已退出。
- 2026-09-29 14:22:17 CST核验：owned PID21160/25568/13228/20540全部不存在；本轮ComputerUnicode计划任务与Host计划任务无残留；8399无监听。
- 清理日志：`.agents/runs/unicode-cleanup-verification-20260929.log`；保留全部测试证据，不删除失败样本，不操作其他用户程序。
