# Desktop feedback v1：本批实现契约

最新用户要求：源码由gpt-6.1-sol subagent开发。保留工作区，不worktree、commit/push，不进行真实GUI输入/启动窗口/SSH。本文细化已确认的可选反馈设计，独立Rust库与平台renderer按此对接。

## 边界及默认安全策略

- 输入内核不依赖反馈。未启用时不spawn、不找renderer文件、不请求新UI权限。
- 明确启用：启动/握手失败拒绝本次Host服务启动，报结构化diagnostic。运行中renderer丢失/协议损坏/超时，默认请求终止当前Host控制权并取消清理，不能静默继续并宣称可见。此策略仅对显式开启生效。
- UI stop是control请求，不自行注入输入；Host验证session+generation后使控制权失效、取消和清理。旧代次stop拒绝。序号sequence只用于展示排序，不充当授权。
- 子进程stdin/stdout为私有NDJSON管道，不监听网络；stdout仅协议，stderr仅无敏感诊断。消息上限16KiB；读行不可无界分配。Host渲染发送不阻塞执行线程，latest snapshot可合并；控制队列有界。渲染器不能因stdin堵住主UI线程。
- 不发送文字输入正文、key内容、凭据、截图、用户应用标题。

## 消息（字段及snake_case锁定）

Host->renderer：
```
{"type":"snapshot","version":1,"sequence":1,"session":{"id":"session-...","generation":1},"phase":"idle","cleanup":"not_needed","surface":{"id":"macos:1","version":"v1","x":0,"y":0,"width":1512,"height":982},"pointer":null}
```
- `session`/`surface`/`pointer`可为null。session字段id非空，generation无符号整数。
- sequence单调，旧帧忽略；display surface单位是Host原生input单位（Mac global top-left CoreGraphics points；Win PMv2虚拟屏像素），不得当截图像素。
- phase：starting / idle / observing / executing / paused / stopping / faulted / closed。
- cleanup：not_needed / pending / released / failed / unknown。
- pointer非null为`{"x":100,"y":100,"kind":"move"}`，kind只允许move/click/drag；仅代表确认派发的输入事件，非Agent计划。
- closed+failed/unknown不得显示安全完成。无session时停止按钮禁用。

renderer->Host：
```
{"type":"ready","version":1,"capture_exclusion":"requested","pointer_feedback":true}
{"type":"stop","version":1,"session":{"id":"session-...","generation":1}}
{"type":"heartbeat","version":1}
{"type":"error","version":1,"code":"..."}
```
- ready在本机GUI创建且平台初始化实际成功后才发。--self-test/协议纯测试不能构造真实ready冒充窗口。
- capture_exclusion仅supported-requested状态`requested`或`unsupported`，**requested表示API设置成功，不代表实机模型截图排除验证通过**。不允许把透明/鼠标穿透当排除。
- heartbeat每秒（具体timeout由Host集成确定，先5秒）。ready前任何stop无效；处理超大/无效帧必须诊断，不原样回显内容。
- ready之后不可改变版本或重复ready。主窗口被关应发stop而非悄悄退出。stdin EOF退出自有renderer，不注入输入、不报告Host已清理。

## 样式

默认紧凑状态条（明确AI控制状态+停止按钮）、轻量pointer ring；装饰点击穿透不激活，停止有独立命中区域。只提供--accent '#RRGGBB'与--label短文案配置（长度边界与转义），不改全局系统光标。特殊动画后置。初始UI未收到有效session不显示“正在操作”。平台capture排除缺口需如实报告给Host集成，不绕过权限，不通过像素涂抹/隐藏-截图竞态假实现。
