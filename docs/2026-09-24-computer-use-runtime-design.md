# Computer-use Runtime：通用视觉计算机操作环境

日期：2026-09-24  
状态：已获继续实施授权；跨平台与独立 Claude Agent 验收目标已纳入  
项目边界：仅 `rpa-computer/`；不依赖父目录的其他项目

## 1. 产品定义与本次收敛

本项目提供让已有 Agent 通过截图和键鼠操作计算机的能力。在 API、CDP、a11y 无法覆盖任务时，视觉通道仍可独立工作。

典型任务：“打开计算器，计算 10×20，告诉我结果。”必须真实打开应用、输入表达式并观察结果，不能以模型心算代替 GUI 操作。

“兜底”指减少对应用专用接口的依赖，不保证所有应用、权限边界和桌面状态都可操作。

用户已指定本机安装且配置好模型的 Claude CLI 作为验证 Agent。因此：

- 不自建 Agent，不重新接入模型 API，不读取、复制或重配用户的模型密钥。
- 不把 Reference Runner、Model Bridge、Python Client 当成第一版交付物。
- 核心为可嵌入的 Computer Runtime；第一种对外入口为 stdio MCP。
- 首个端到端验证使用当前本机 macOS 桌面；随后按用户新授权使用 ssh acer-win 验证 Windows 产物，不继承父项目服务。
- 根据后续明确授权，本次目标同时包含 macOS 与 Windows 编译产物及真机验证。Windows 采用交互式 Host + 受保护的远程测试入口；不依赖父项目服务。HTTP 客户端仍非必需。

本地检查记录：2026-09-24，`uname -s` 返回 Darwin；`claude --version` 返回 `2.1.251 (Claude Code)`；CLI 帮助提供 `--mcp-config`、`--strict-mcp-config`、`--tools` 等选项。这仅证明本地命令与配置入口存在，不代表模型连接、图像读取或 GUI 任务已通过验证。

## 2. 边界与架构

```text
用户任务
  ↓
Claude Code（已有 Agent 与模型配置）
  ↓ MCP tools / 图片结果
rpa-computer MCP 进程（stdio；不是新的 Agent）
  ↓ 类型化调用
Computer Runtime
  ↓
Platform Backend → 当前桌面
```

### 2.1 Computer Runtime

负责会话、操作权、观察、坐标变换、动作校验、执行串行化、输入清理、取消和准确结果。

不负责模型调用、任务规划、理解“确定按钮”等语义目标、选择下一步动作、判断业务完成或重试有副作用的操作。

### 2.2 MCP 接入层

把 Runtime 的接口公开为少量 MCP 工具；将截图作为图像内容返回，同时提供观察 ID、尺寸和执行状态。

它可以调用嵌入的 Runtime，不要求另外启动 Host daemon。stdio 的 stdout 仅传 MCP 协议，诊断输出走 stderr 且不包含敏感输入。

MCP 层不暗中调用模型、不自行寻找屏幕元素、不拼装隐藏 Agent 循环。MCP 的具体 SDK、协议版本及图片内容编码在实现前依据对应官方协议核查，本规格不锁依赖版本。

### 2.3 已有 Agent

Claude Code 负责把任务与截图交给模型，决定下一步工具调用、管理任务历史、请求用户确认、结束或报告失败。

本项目提供工具说明与验收任务，不接管 Claude Code 的内部上下文压缩、模型切换、对话存储或任务状态机。

### 2.4 薄验证脚本

仅负责编排一次 Claude CLI 启动、传入项目 MCP 配置、限制工具范围、收集脱敏指标和检查运行结果。不自己写“请求模型—解析响应—执行动作”的循环。

因此它不是之前的 Reference Runner。以后如需演示直接模型 API 接入，可作为可选 example 增加，不进入核心依赖。

## 3. 首版范围与工程组织

保留 Rust 作为当前 Runtime 与 MCP 进程的工程起点；这一选择来自现有项目，不是 computer-use 的协议要求。先在一个 crate 内按职责组织，只有出现独立依赖或发布需要时才拆 workspace。

建议目录：

```text
rpa-computer/
  src/
    runtime/        # session、observation、action、result、validation
    backend/        # trait + macOS 实现；未来 Windows
    mcp/            # 工具、图片结果、连接生命周期
    bin/            # stdio MCP 入口
  examples/         # 项目级 MCP 配置示例
  tests/            # contract、fault injection、真机场景
  scripts/          # 可选 CLI 验证入口，不包含 Agent loop
  docs/
```

类型和约束必须有单一事实来源；MCP 动作 Schema 尽量从类型定义生成，另外做运行时范围、状态和能力校验。不手写互相漂移的三套类型、Schema、提示词。

第一版：本机、单个固定主屏目标、单个控制会话、截图和基础键鼠、输入安全清理、MCP、Claude CLI 验证。

暂不做：模型训练、应用专用操作、自动 API/CDP/a11y 路由、多 Agent 并发控制同一桌面、多屏拼接、视频流、工作流录制回放、通用远程部署平台、完整聊天 UI。本次仅增加测试 Windows 所需的最小受认证 loopback 传输与交互式启动。

## 4. 核心接口与 MCP 工具

Runtime 的操作：

| 操作 | 责任 |
|---|---|
| describe | 环境类型、能力与权限预检结果 |
| open_session | 绑定操作目标，申请控制租约 |
| observe | 获取当前画面，可有界等待后重新截图 |
| step | 基于一个观察执行一个动作并返回后续观察 |
| get_step | 查询既有请求结果，不重新执行 |
| pause / resume | 暂停输入；恢复后需重新观察 |
| close_session | 停止、清理、关闭，幂等 |

MCP 映射为 `computer_describe`、`computer_open`、`computer_observe`、`computer_step`、`computer_get_step`、`computer_pause`、`computer_resume`、`computer_close`。内部调度、租约维护、图像缓存不作为模型工具暴露。

observe/step 正常返回结构化元信息和图像内容；只给截图路径、资源 ID 或 base64 文本，不算完成了模型可见的图像接入。第一项接入测试必须证实当前 CLI/模型真的收到并理解了图片。

step 参数示意：

```json
{
  "session_id": "session-example",
  "request_id": "request-example",
  "based_on": "observation-example",
  "action": {
    "kind": "click",
    "position": [120, 80],
    "button": "left",
    "count": 1
  }
}
```

MCP 只是接入方式，Runtime 不依赖 MCP 类型；以后可增加嵌入 API 或独立远程适配层，而不改动作和执行结果语义。

## 5. 会话与控制权

Session 保存 session_id、runtime_instance_id、OS 用户会话标识、固定 surface_id、能力、输入持有状态和预算。

- 同一桌面只允许一个本项目控制租约；不同 MCP 进程也必须共享 OS 会话级互斥。
- 本项目的互斥不能阻止用户或其他程序操作电脑，不宣称独占整台机器。
- 表面在打开时绑定，主屏变化时不能静默换目标。
- 会话状态：Opening、Ready、Executing、Pausing、Paused、Closing、Closed、Faulted。
- Paused 拒绝输入；恢复后废弃待执行决策并重新观察。
- 清理失败或几何失效进入 Faulted，只接受诊断、清理、关闭；首版修复环境后重新建会话。

stdio 生命周期：正常连接关闭/EOF 时触发清理；进程崩溃无法保证 release 全部成功，重启不得宣称旧动作未发生。等待模型期间不按工具空闲时间直接撤销会话，使用本地可见的总会话预算；初始上限 10 分钟，不能由模型扩大。

单步 deadline 初始 15 秒，输入动作阶段最多 5 秒，观察等待最多 3 秒。控制路径不能排在动作队尾；pause/close、连接关闭、本机停止入口均可通知执行取消。

停止阻止新任务输入并尽力释放本会话按下的键鼠；已发出的点击和文字不能回滚。释放失败必须可见，不能报告“已安全停止”。正常后端路径目标：收到停止后 1 秒内不再发送新的任务输入，清理 release 除外；后端卡死单独验证故障报告。

## 6. Observation 与坐标

Observation 至少包括：

```text
observation_id, session_id, surface_id
geometry_version, input_sequence
captured_at, capture_duration_ms
image { mime_type, width_px, height_px }
valid_image_rect, image_to_input_transform_id
```

约束：

1. 图像声明尺寸与最终编码图片的实际解码尺寸一致。
2. 模型动作使用本次返回图像的像素坐标，左上角为原点；不等同于 OS 逻辑点或后端输入坐标。
3. 原始截图像素、模型图像像素、后端输入空间分别保存；转换只在 Runtime 的执行边界处理。
4. 第一版等比缩小，不拉伸、不留黑边，按真实输出确定尺寸；不承诺固定 1366×768。
5. MCP 层不再次裁剪/缩放。CLI 或模型服务是否对图片预处理属于接入验证的一部分，不能凭工具返回成功就假定坐标正确。
6. 执行前核对 surface 和 geometry；屏幕配置变化时拒绝旧动作，进入 Faulted 后重新建会话。
7. 接受可能产生输入的动作时递增 input_sequence；即使部分失败，也使先前观察失效。
8. 只读观察可共享输入序号；step 的依据必须与当前序号一致，且在 freshness 限制内，初始值 120 秒。

这些规则防止本项目用错会话、几何和旧输入状态，不能阻止用户操作、弹窗或动画造成的界面变化。

## 7. 动作与准确结果

首版动作：

- click(position, button, count)：count 1–3。
- move(position)。
- drag(path, button, duration_ms)：整体执行并负责释放。
- scroll(position, delta_x, delta_y, unit)：明确单位；仅接受后端声明支持的单位，不静默近似转换。
- text_input(text)：Unicode 文本输入，不用它实现快捷键。
- key_chord(modifiers, key)：先完整解析，再按键，最后释放。
- key_hold(key, duration_ms)：有界按住单键。

坐标必须有效且不越界；拖拽 2–256 点；文本最多 4096 个 Unicode 标量；单轴 scroll 绝对值最多 100 wheel ticks（后端若支持其他单位须另有上限）；hold/drag 最多 5 秒。限制在 capability 中公开，模型不能自行扩大。

首版不向模型开放跨调用 mouse_down/up 或 key_down/up；后端仍需内部 press/release 状态管理。Screenshot 是 observe；Wait 是有界 observe 等待；Done 不属于设备动作。

step 默认固定 settle 延迟 300ms 后截图，未来可增加有上限的画面变化采样。不把等待结束或画面稳定解释为业务完成。

StepResult 分开描述：

| 维度 | 值 |
|---|---|
| input_outcome | not_started / dispatched / partial / unknown |
| observation_outcome | available / failed / skipped |
| cleanup_outcome | not_needed / released / failed / unknown |

附带 request_id、耗时、结构化错误、取消标识、已执行子事件范围和可用 Observation。

输入完成但截图失败，必须保留 dispatched，不包装成“未执行”；partial/unknown 不能触发 MCP 层自动重放。dispatched 只说明输入 API 报告完成，不保证应用已响应或任务成功。

基础错误：invalid_action、unsupported_action、stale_observation、geometry_changed、permission_denied、desktop_unavailable、lease_conflict、cancelled、deadline_exceeded、input_error、capture_error、cleanup_error、resource_limit。

同一 session/request_id 在当前进程生命周期内去重；相同 ID 不同请求体拒绝。首个输入前登记请求，最多保留 1000 个请求，达到容量上限拒绝新步骤，不逐出仍可重试的记录。跨崩溃不承诺 exactly-once，旧会话结果未知时重新观察，不盲重播。

## 8. Claude CLI 接入与验证隔离

### 8.1 使用已有配置

沿用用户已经配置的认证和模型，不指定新模型、不配置自动 fallback、不创建第二套 API Client。项目只提供 MCP 配置示例和测试任务。

通过一次 CLI 启动加载项目 stdio MCP 配置，不默认写入用户全局 MCP 设置。入口配置使用 command/args，路径由项目内构建结果确定，不依赖父项目路径。

启动参数以执行时本机 `claude --help` 为准；已确认可用的方向：

- `--mcp-config`：加载本项目配置。
- `--strict-mcp-config`：排除其他 MCP 配置。
- `--tools ""`：按本地帮助禁用内建工具，实际工具清单需在测试时核对。
- `--allowedTools`：仅对本项目实际工具名配置必要许可，不绕过所有权限。

`--strict-mcp-config` 不等于完全禁用了插件、hooks、记忆或继承指令。验证前还需核对实际可用工具和项目外上下文；若不能同时保留认证并隔离这些因素，明确记录污染因素，不能把该轮当成严格视觉通道验收。

不默认使用 `--bare`：本机帮助显示它会改变认证和配置加载行为，可能破坏用户现有连接。也不使用 `--dangerously-skip-permissions` 作为默认方案。

### 8.2 先验证图片链路

顺序：CLI 能发现工具 → 调用截图工具 → 模型能识别受控测试画面 → 对同一画面的靶点执行坐标操作 → 读取后续画面。

若已配置的模型/代理链不能传递图片，报告明确阻塞并请用户处理，不自动换模型、不把截图描述成已经被模型看见。

### 8.3 防止绕过视觉通道

计算器验收中禁用 Bash、代码执行、浏览器专用工具和其他 MCP，不允许通过 AppleScript、应用 API、文件读写或算术脚本代替图形界面操作。

任务指令要求先观察、仅通过本项目工具操作、完成后根据截图核对。仅在提示词里要求“不作弊”不够，必须检查实际工具调用轨迹。无法确认轨迹或存在旁路操作，计为无效测试而不是成功。

同一模型提出动作并判断结果可能误判；保留用户允许的最终截图进行人工核对，不称为独立确定性证明。

### 8.4 验证脚本不是 Agent

脚本只启动 CLI 和监控预算，不介入模型推理循环。墙钟超时、CLI 退出或用户停止时，通知 Runtime 关闭会话并报告清理结果；不能只杀掉 Agent 就宣布桌面已安全。

第一轮使用交互式 CLI，便于授权和观察；非交互式 `--print` 验证在工具许可、停止路径及图片链路证明可用后加入。不把日志输出里的模型“成功”当作验收结论。

## 9. 交互、安全与隐私

不另造聊天界面，复用 Claude Code 展示工具调用和回答。本项目仍须提供与 Agent 独立的本机停止入口及清晰的会话状态，不能依赖模型主动调用 close 才能停止。

首版本机停止快捷键可配置；注册失败需预检报错。用户可 pause 后人工接管，resume 后必须重新观察。工具结果区分“停止已请求”“输入已停止”“清理完成/失败”。

权限预检区分屏幕采集与输入控制；不足时给出本机处理说明，不请求自动提权、不尝试绕过系统权限。首轮截图与输入验证前让用户知道将操作当前桌面，并要求关闭敏感画面。

安全与隐私约束：

- 屏幕文字是不可信环境数据，不能改变用户任务授权；工具说明提醒 Agent，但不承诺单靠提示词消除注入风险。
- 测试使用无敏感数据的应用/文档；付款、删除、发送等高影响操作需外部 Agent/用户确认，Runtime 不声称能理解任意坐标点击的业务含义。
- 截图经 Claude CLI 发送到用户配置的模型服务，测试前说明；本项目不选择其他服务商。
- Runtime 默认不持久化截图或明文输入；图像缓存初始总上限 256 MiB，固定保留当前观察及执行依据，容量不足报告 resource_limit，关闭会话清理。
- 事件记录动作类型、状态和时间；text_input 默认只记长度，不记正文。
- Runtime 的“不持久化”不约束 Claude Code 或模型服务的会话/日志策略，不能据此宣称端到端无留存。
- 最终截图导出需显式允许；未导出时关闭会话后旧证据不再可取，完成说明应明确这一点。

## 10. 测试与验收

### 10.1 确定性测试

- 纯逻辑：坐标变换、真实图片尺寸、Schema 与动作一致、参数限额、状态机、请求去重、日志脱敏。
- 假后端：在移动、press/release、截图等节点注入失败，验证 partial/unknown、清理、取消、故障后拒绝输入和无盲重试。
- MCP 契约：工具 Schema、图片结果、结构化错误、stdout 无日志污染、连接关闭触发清理、重复请求与并发调用语义。
- 同一桌面的输入及操作后观察持有同一调度权，避免本项目动作交错；取消控制不受该长操作阻塞。

### 10.2 本机真机场景

| 场景 | 通过条件 |
|---|---|
| 图片识别 | Claude CLI 使用当前模型识别受控截图中的内容，而非工具文本透露的答案 |
| 靶点定位 | 模型基于截图点击受控目标，实际命中与图像坐标一致 |
| 计算器 10×20 | 真实启动与操作；最终画面可核对结果 200，没有应用专用执行代码 |
| 文本编辑 | 在本机文本编辑器中输入中文、换行并修改；仅视觉与键鼠，不依赖保存文件 |
| 快捷键 | 触发快捷操作而不是插入文本；无效组合在输入发生前拒绝 |
| Retina/缩放 | 图像声明与实际尺寸一致，不把图像像素直接当 OS 输入坐标 |
| 几何变化 | 分辨率/显示目标改变时旧动作拒绝执行 |
| 取消与失败 | hold/drag 中取消后不再继续任务输入；清理失败可见且进入 Faulted |
| Agent 退出 | 正常断开触发清理；异常退出的未知状态不能被包装成安全完成 |
| 权限不足 | 清楚区分截屏与输入权限，拒绝执行并引导处理 |

Runtime 的确定性契约测试必须全部通过。Agent 端计算器和文本编辑各 10 次，试用目标各至少 8 次真实成功，0 次无有效证据宣告成功、0 次未报告的输入残留。先验证链路再开始统计，记录 CLI 版本、实际模型标识（不含凭据）、系统状态、耗时和失败类别；20 次小样本不代表普遍成功率。

## 11. 分阶段交付

### M1：本机 Runtime + macOS 后端

重构会话、观察、坐标、动作、结果、取消、清理和预检；使用假后端及确定性脚本验证，不调用模型。

### M2：stdio MCP + Claude Code 最小闭环

增加 MCP 工具与项目级配置示例；验证已配置模型的图像链路，随后执行真实计算器任务。无需 Reference Runner、Model Bridge、HTTP Host 或 Python SDK。

### M3：可试用与故障验收

完善本机停止、权限引导、CLI 隔离、文本编辑任务、故障注入、取消/退出/Retina 测试、明确的留存说明与质量记录。

M1–M3 共同构成可试用第一版；“模型回复 200”或单次点击成功不等于完成。

### 后续扩展

Windows 后端、交互式 Host 及 ssh 隧道测试已升级为本次交付要求，其协议/认证/生命周期设计见 `.agents/CONTRACT.md`（相对项目根目录）。其他 Agent/MCP 客户端、原生模型 Adapter 或直接 API 示例仍属可选后续扩展。两平台由同一 Runtime 契约约束，不以 macOS 成功替代 Windows 验收。

## 12. 旧设计与验证状态

旧的 toolkit 文档已删除；本文件为唯一设计基线。上一版引入的远程 Windows、父项目服务、Python Runner 和模型调用链不再是首版前提。

仍然保留：观察绑定动作、坐标空间分离、单写者输入、结构化副作用结果、无盲重试、可取消与输入清理、真实画面完成证据。

当前只执行了 CLI 版本/帮助和平台查询，没有请求模型、启动 MCP、采集屏幕或注入键鼠。现有模型是否接受 MCP 图片尚未验证。

用户已授权继续实施：协调者负责设计、调度与代码审查；所有产品实现、测试及部署脚本交由 Claude Code Agent 完成。计划与分工在 `.agents/`。不更改全局 Claude 配置，不提交或推送。

## 13. 多 Agent 实施与无源码验收

- `.agents/README.md` 定义角色，`CONTRACT.md` 固定协作接口，`plans/implementation.md` 维护完整目标门禁。
- 实现与白盒 QA Agent 可以读取源码；真实 GUI 验收 Agent 在仓库外启动，只获得编译产物、MCP 配置和任务。不能继承实现者会话或把源码摘要放进验收提示。
- 除禁用 Read/Bash/Glob 等工具外，用操作系统文件沙箱阻止验收 Agent 读取原始源码路径，并执行负向读取探针；单靠提示词不算隔离。
- GUI 工具仍具备视觉访问当前用户桌面的能力；工具白名单与文件沙箱不等于阻止所有经桌面发生的间接读取。使用受控测试桌面/窗口、禁止打开源码/终端的验收任务，并审查轨迹；发现旁路则测试无效。若无法建立足够隔离，必须报告该门禁未满足。
- macOS 和 Windows 分别记录编译目标、产物哈希、权限预检、MCP 图片链路、实际操作和最终画面；不自动接受模型成功声明。
- Windows Host 必须和用户 explorer 处于同一交互式会话，通过 Interactive Scheduled Task 启动；不得在 SSH 服务会话直接执行 GUI 控制。
- MCP stdio 仍为本机入口；Windows 远程测试使用 loopback-only 受认证传输经 SSH 转发，凭据不进入 Agent 上下文和日志。实现契约以 `.agents/CONTRACT.md` 为准。
