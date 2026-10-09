# 实施任务：Host可选反馈接线（Codex+gpt-6.1-sol；CC负责测试）

工作目录本项目。阅读最新 `.agents/README.md` / `.agents/CONTRACT.md` 尾部角色规则及 feedback设计、v1-wire任务。不要worktree、commit/push、真实GUI/截图/SSH。源码/测试代码可以写，测试执行归CC+GLM；只允许开发所需compile check，不把它当验收。

## 目标与约束

让已有独立 `crates/desktop-feedback` 与 `desktop-feedback` 默认renderer真正可选接入Host。保持native-input完全无UI依赖。不启用时零子进程/零新权限/无需renderer文件；显式启用失败清楚报错，不悄悄当未启用。

1. Host添加明确CLI配置（建议 `--desktop-feedback <可执行文件>`、`--feedback-accent`、`--feedback-label`），类型解析/帮助/describe/remote supervisor向owned stdio child透传完整。无shell拼接，既有TLS/token隐私不变。默认off；远程Mac控制Windows反馈必须运行于Windows child所在交互桌面。
2. `src/feedback/**`独立适配库：有界private pipe reader、latest snapshot writer、监督/heartbeat、协议严格验证。慢/堵renderer不得阻塞输入线程、也不得无限堆积任务/字节。IO退出/kill/reap有界且不得遗留自有worker/子进程。利用已有feedback crate，避免另写重复状态机。
3. Runtime是session事实来源，snapshot来自实际状态而非请求名猜测。open成功授予session+monotonic generation，observe/capture与实际input阶段准确；预校验拒绝不得报告执行了点击。输入pointer/click只来自成功的实际dispatch，不包含文本/按键/应用标题/截图。可以用Backend decorator感知capture/inject，但生命周期仍需Runtime接线，错误/cleanup结果真实。无反馈路径接口保持自然可用。
4. Stop是同步撤销控制权，不是hide或一次cancel：IO收到经session+generation验证的stop，先不可逆地撤销当前Host child的控制权，再调用现有cancel+shutdown，拒绝旧/已排队的新动作及open/resume重清cancel绕过；通过现有worker epoch机制防止入队/dispatch/native之间竞态。旧会话stop不得停新会话。允许外部supervisor在显式新连接下再授权，不承诺永久屏蔽合法未来连接。暂停/close与终止区分。renderer失败（显式启用）默认同一安全终止流程。
5. UI stopping必须等实际release结果：released/not_needed才安全closed；failed/unknown明确faulted，既有worker shutdown_unknown/quarantine不能改成成功。收到/发出UI消息成功绝不等同原生释放成功。必要时保留故障指示短时或stderr准确诊断，但不无限挂Host。
6. Ready capture_exclusion=requested只代表平台API设置成功；实际截图排除另有验收。对renderer明确unsupported的当前capture组合，显式启用必须拒绝或准确能力错误，不把反馈层送给模型却假称排除。macOS现有capture走screenshots，平台renderer会报告需要适配之处；不要猜测透明/sharingType即可满足。不在本任务中修图或对截图像素涂抹。

## 推荐写集（正式分配后以消息为准）
Cargo.toml/lock、src/lib.rs、src/feedback/**、src/runtime/runtime.rs及新独立feedback测试模块、src/mcp/worker.rs及worker/native.rs（现有worker/tests.rs由原子输入任务暂占，等释放才动）、src/bin/computer-host.rs、src/mcp/remote/host.rs及必要RemoteArgs测试适配、README反馈使用说明。不得改native-input、fixture、独立feedback crate、renderer（若依赖缺口报告协调者转交对应owner）。不要向已近1000行的session.rs再堆展示逻辑；新增适配模块。

## 编写交付测试（由CC运行）
- off模式不存在renderer文件仍正常，无spawn；显式enable路径缺失/无ready/unsupported明确失败。
- capture vs inject vs拒绝 vs idle/paused状态序列，秘密文本不进入wire，真实callback验证不是仅请求名映射。
- 停止长操作取消与release；后续旧动作/open/resume拒绝；旧代次stop不影响新session；stop与open/resume同时、queue/native间隙race；release失败/unknown不假成功。
- ready/heartbeat/EOF/protocol过大/错误/慢消费者失联不阻塞输入，有界shutdown且重复stop幂等。
- remote supervisor完整CLI透传，TLS/token不进入renderer。
- fake可执行renderer/纯mock测试可以作为CC的测试环境，不冒充GUI实测。

报告文件 `.agents/reports/desktop-feedback-host-sol-20260930.md`：改动列表/公共API/编译检查/待CC精确命令/平台缺口。不能说测试通过或GUI已验收。
