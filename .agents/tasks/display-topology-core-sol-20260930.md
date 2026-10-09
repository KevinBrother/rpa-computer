# 实施：独立多显示器拓扑/区域映射基础库（非GUI验证）

源码实现者：Codex+gpt-6.1-sol；测试执行统一CC+GLM。直接编辑 /Volumes/doc/workspace/datagrand/rpa/rpa-computer，新独占写集 `crates/display-topology/**` 和 `.agents/reports/display-topology-core-sol-20260930.md`。不改root Cargo/src（Host反馈正在实施）、native-input、capture库、renderer、fixture。不worktree/commit/push，不GUI输入/截图/SSH，不执行测试，不启动其他agent。必要cargo check编译允许；测试代码编写后交CC。

先读docs/superpowers/specs/2026-09-30-complete-computer-actions-design.md多屏部分和docs/computer-use-acceptance-cases.md多屏12项。只读当前Geometry/CoordMap了解旧单屏设计，但新库不依赖root，也不是假的显示器枚举器。

## 本批精确定义

交付纯Rust独立workspace（std优先，必要serde允许），为后续真实backend枚举/capture组合/Runtime区域映射提供生产会使用的逻辑：

- DisplayDescriptor：运行期不透明id、is_primary、native input bounds（i32 origin/u32 size）、capture像素尺寸、有限正scale与rotation等使布局/DPI可变化的事实。native bounds不能换算成截图像素；Mac单位points、Win单位PMv2 pixels，单位由调用方明确传入而非库猜系统。
- Selection：Primary / Display(id) / Desktop。显式副屏不存在不得fallback；空列表/重复ID/非法尺寸/溢出/无主屏或多主屏精确错误。
- TopologyTracker：按稳定ID排序的完整布局事实做精确比较，枚举顺序变化不算拓扑变更；position/size/capture-size/scale/rotation/primary/插拔任一变化都递增checked generation。不要拿可能碰撞的短hash冒充唯一布局身份；明确tracker lifetime，generation耗尽报错。快照携带该generation，Display id不承诺跨重插稳定。
- CapturePlan：primary/explicit单屏和desktop计划。全桌面视觉布局保留原生屏相对位置/空隙；每个tile带显式native bounds及composite pixel rect/来源capture尺寸，不能套一个宽高比来忽略各屏DPI。可把各屏原图重采样到统一全局native布局密度，但必须输出每屏region映射，单屏模式保留各自细节。
- 限制由显式CaptureBudget给入(max_width,max_height,max_pixels，非零、乘法overflow验证)，计划最多使用预算；无巨大稀疏布局任意分配。库只算plan，不分配像素/不截图。量化后的零宽tile、overlap或歧义不可悄悄通过。重叠布局/镜像无法安全支持时明确返回diagnostic并记录后续限制，不能silent primary；若可以给出无歧义处理规则请写明并测试。
- ObservationMapping：从实际观察PNG宽高缩放plan的regions，再image->native，坐标边缘严格夹在对应display有效像素/point范围，负原点安全，拒绝越界/blank gap/padding/旧generation；绝不因round把右下角点到相邻屏或屏外。输入native coords仍必须落在选中display。metadata与真正像素裁剪/缩放协议清楚，后续backend按该plan组合。
- Drag：用户指定的waypoints必须每个命中真实display，显式空隙点拒绝；跨屏两个合法端点允许，内部连续插值可以沿原生直线跨过物理gap，但不能把gap可插值误用为允许click gap。提供与click不同的明确内部segment/native interpolation API或计划（不要把drag压入driver整段sleep）。Runtime继续掌管时序/取消并按topology generation变化停/清理。
- 所有坐标/预算/代次错误结构化；不用 saturating/clamp 把非法请求改成可点击目标；只有合法点的最后量化可做防越界处理。

## 测试代码与交付

写纯测试覆盖左右负坐标、上下布局、混合DPI、gap、边界rounded风险、selection缺失、旧generation、排序稳定、插拔/旋转/primary/DPI变更、极大bounds/像素预算/算术溢出、跨屏drag合法及用户gapwaypoint拒绝。不能运行cargo test，由CC执行。不要用同一函数产出expected再测自己。

只做开发cargo check --all-targets与必要format（不能声称验收）。README写公开API/真实backend后续接线方式、截图plan约定和限制；报告changed文件/编译结果/待CC命令。不宣称多屏端到端已实现，真实枚举/合成/Runtime接线是下一任务。
