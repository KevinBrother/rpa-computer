# Windows 多屏：root 接线与验收边界

本文描述 root StageB 源码契约，不是 Windows 实机验收报告。实现者只执行 Windows target `cargo check`；测试执行、真实枚举、截图、输入、Stop 与排除反馈窗口的验收由 CC 单独进行。Mac/Linux 验证暂停。

## API（仍为原有 8 个工具）

`computer_describe({})` 在 backend 所属线程查询当前拓扑，不截图、不输入。成功时新增：

- `displays[]`：真实 `id`、`is_primary`、`native_bounds`、`capture_size`、`scale`、`rotation`。
- `native_unit`：Windows provider 明确为 `physical_pixels`；不从编译目标推断 mock 的单位。
- `topology_generation`：包含 tracker authority 和 revision 的 opaque 字符串；不能只比较 revision，也不能跨 Host 实例复用。由库正式 `Generation::to_token()` 无损编码完整 authority+revision：`topology:tracker:<16位小写hex>:revision:<16位小写hex>`，固定59字节ASCII，满足既有feedback版本字段约束。不是Debug、标点过滤、截断或哈希；调用者按opaque token比较，不作为跨进程永久身份。
- `display_topology`：包含上述三项的对象；`display_selections` 表示实际 backend 支持的 selector。

枚举失败时 `available:false`、`display_topology:null` 和 `preflight_error` 说明错误，不回退至旧列表。legacy primary-only backend 没有完整拓扑时会明确返回 null 和说明，而不是伪造 generation。

`computer_open` 示例（下面是工具参数，不是可执行命令）：

```json
{"display":{"kind":"primary"},"max_width":1366,"max_height":768}
```

```json
{"display":{"kind":"id","id":"<computer_describe 返回的完整 display ID>"}}
```

```json
{"display":{"kind":"desktop"},"max_width":4096,"max_height":4096}
```

省略 `display` 等价于 primary。只接受 `max_width`、`max_height`、`display`；selector 的未知 kind、额外字段、缺失/空 ID 在 backend 查询前拒绝。ID 不存在报错，不改选 primary。每轴默认最大图像 1366×768；用户上限仍为每轴 4096，最终图像可以因像素/字节预算更小。

session 绑定选择和**完整拓扑** generation，包括未选中的显示器。几何、源像素尺寸、DPI/scale、旋转、primary 或枚举成员变化会失效；`resume` 也检查。失效需 close 后显式 fresh open，不自动改选。反馈 Stop 撤销的是该 Host child 的控制权，不能用 fresh open/resume 绕过。

## 图像坐标与输入

observation（包括 step 的 observation）在原图像尺寸字段之外携带：

- `native_unit`、`topology_generation`；
- `selected_display_ids[]`；
- `mapping_regions[]`，每项含 `display_id`、`native_bounds`、`image_rect`。

`image_rect` 对应实际返回 PNG 解码后的像素尺寸，不是请求上限。Windows 使用冻结 `CapturePlan`/`ObservationMapping` 的逐显示器分区构图，按真实计划缩小；不会对总 bbox 用单一比例执行混合 DPI 输入。单屏 region 也使用同一映射契约。源截图尺寸和 native 输入尺寸单独保留。

用户指定的 click/move/scroll position 和所有 drag waypoints 必须位于真实映射区域内。黑色 gap、padding、图像外位置在输入前拒绝。跨屏 drag 用库 `DragPlan` 从合法 waypoints 生成 native 中间采样：**内部轨迹可以经过 gap，用户目标不能指向 gap**。逐 atomic event 查询 generation；每次 drag move 也按最新 generation 校验库采样。查询错误或拓扑变化中止后续输入，执行 best-effort release，保留 partial / cleanup failed，而不是假报回滚成功。

拓扑查询返回后再次检查 cancel 和 input deadline。同步 native 查询/捕获本身无法被本线程立即打断；既有 worker timeout/quarantine 仍是处理卡住 native call 的边界，不能声称 Stop 撤回了已经进入的 native call。

## Windows 产品路径

`worker/native` → `DesktopBackend::new` → 既有 PMv2 `prepare_thread` → `GdiProvider::attach` → 持久 `DisplayProvider` / `DisplayBackend` / tracker。

geometry、selection 和 capture 都使用该 provider。Windows 不再调用旧 primary `screenshots` 捕获函数。捕获有预检查、库内 pre/post 枚举和 root publication 前 generation/cancel 检查；失败不返回旧图冒充当前图。其他平台维持 primary-only 旧路径，id/desktop 明确 unsupported；本轮没有其编译或运行证据。

`FactBackend` 转发完整拓扑、selection、mapped capture 和错误。反馈 surface 单屏用真实 ID/bounds，只有整个桌面用 **`Surface.id = "desktop"`**，bounds 是全部所选 native bounds 的并集。内部 gap transit 不发布新的 ring 目标；confirmed pointer 仍在 native inject 成功后才更新。wire v1、renderer 和 native-input 本轮未改。

## 资源及交付上限

- 输入行上限维持 1 MiB，输出行上限维持 64 MiB（serialized JSON gate 预留换行 1 byte）。
- provider source buffer 上限 64 MiB、库 engine peak budget 192 MiB；超限拒绝，不提高 native 源预算。
- 单 PNG 上限 16 MiB；root admission 对所有可能布局约束 output pixels 为 `(16 MiB - 128) / 6`，且使用库 stored-DEFLATE PNG estimator 做计划预检。布局查询竞态不能换用一个不受限制的后续计划。
- session 图像缓存维持 32 MiB；返回的 PNG 仍受实际字节数检查及既有 cache admission 约束。不能为了缓存成功删除正在受保护的 ledger/basis 语义。
- MCP 在 base64 分配前检查 PNG/base64 预算，在最终 JSON 写出前通过 counting writer 测实际转义后的序列化长度。整帧超限不截断 PNG/JSON，改交付结构化 `delivery_error:output_budget`，`image_delivery_outcome:withheld`。
- 交付失败不抹掉 `input_outcome`、`observation_outcome`、`cleanup_outcome` 或原执行 error；成功派发仍是 dispatched，capture available 和网络 image withheld 是不同事实。

以上是分层 admission/serialization 约束，不是进程 RSS 上限或已测性能保证。PNG decode、cache、base64 与 JSON 可能同时持有不同副本。

## 纯测试与未验证范围

root 新测试使用内存 `FrameProvider` 驱动**生产 DisplayProvider 和冻结库真实 composition/mapping engine**，经 Runtime/FactBackend/MCP 序列化边界断言。输入只记录 vector，release 只计数；不创建 GDI provider、不调用真实枚举或截图、不创建窗口、不启动 Host/renderer、不使用 socket/SSH。deadline 测试只做约 21ms 的进程内 sleep。这些测试源不能替代 Windows 运行或实机证据。

仍需 CC 独立验证：原 6 RED 保留后的 Windows GREEN、新增 seam tests、受影响 root 回归，以及经协调授权的真实多屏/负坐标/竖屏/混合 DPI/热插拔/跨屏 drag/Stop/cleanup/反馈截图排除。renderer 单独 GREEN 不等于 root 实机验收。

库轮询只能发现观测时存在的变化，不能证明两次 query 之间瞬间切换又恢复的拓扑未变；本轮未增加 OS display notification 订阅。GDI 路径不承诺 HDR/ICC、protected/secure desktop、overlay、独占全屏内容可见性。本轮不修改或升级 feedback capture-exclusion 的证据等级，requested 不等于 verified；Mac filtered capture 也未在此接入或宣称可用。
