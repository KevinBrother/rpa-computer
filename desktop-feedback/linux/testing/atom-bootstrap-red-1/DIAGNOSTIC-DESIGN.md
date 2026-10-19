# 最小初始化诊断 seam（设计 ONLY，未实现）

2026-10-16。保留首次固定操作/error class/major-minor/atom NONE；不更改stdout协议、
rc65、Ready时机、EOF优先级、watchdog或退出排空。不得据此重跑GUI。

## 当前证据与精确缺口

原始证据在 `.agents/runs/linux-parity-20261015/cc-ui-live-qualified-1/remote-logs/evidence/run1/`。
已读result/trace/stdout/stderr；该目录artifacts.sha256实际15项均匹配。
唯一真实观察仍是renderer_ui_failed、rc65、无Ready/renderer CreateNotify、stderr空。
connect失败发renderer_native_failed（原66 process_boundary.rs:217–240），因此该观察
与connect成功后Runtime初始化失败相符。observer成功不等于产品facts/property调用成功。
本次首失败X request、返回X error和实际atom值都UNKNOWN；不能写成已实测BadAtom。

固定源码已确认：原66 x11/mod.rs:10–26未指定only_if_exists，xcb1.7.1宏默认true；
原66 x11/facts.rs:43无NONE检查就用desktop作GetProperty.property，:44的缺失属性fallback
不能处理无效atom。workarea与后续configure属性也有同类风险。不是字体/WM/环境猜修。

Primary source（本地锁定vendor，不依赖最新网页）：
- xcb1.7.1/src/lib.rs:468–480，默认true、send-all/wait-in-order、NONE当普通reply返回。
  SHA256 `3b96f7f095d848c2e4fc9e9b97976ca7b7e354087e59e5e8156c50d2a2422cab`。
- xcb1.7.1/xml/xproto.xml，InternAtom opcode16、GetProperty opcode20；不存在+true返回NONE，
  无效property产生Atom error。SHA256 `45a0e10ef7b01346797a352c2c8eec9a90307fb6693f50802072ae2693680802`。

## 推荐：固定首错记录，不增加X探测请求

1. 一个startup-owned、定长、first-write-wins记录，跨Native cleanup保留。字段仅固定enum：
   phase、operation、role、error_class；可选numeric major/minor/error_code；atom_is_none布尔。
   不缓存格式化xcb错误全文，不输出DISPLAY/auth路径/cookie、任意property值、label或用户文本。
2. 在真实操作结果被map_err丢弃之前记录：首先覆盖atom intern、facts的compositor/root geometry、
   desktop/workarea/GetMonitors/resources读取及有限validation guard；create/configure/paint/present
   仅增加固定阶段标记。GetProperty区分desktop/workarea/resources，不能只写“facts失败”。
3. Runtime记录最终失败阶段；startup checkpoint若以transport failure覆盖native结果，诊断分开
   表示first_operation_error和terminal_reason，不错误声称前者赢得退出原因。cleanEOF取消仍
   走原清理，不因保存过native error而发Error/Ready或改rc；未被终止路径采纳的错误不变成失败。
4. 在原failure路径一次性best-effort输出到独立诊断sink/stderr，<=512 bytes；不得用可阻塞的
   eprintln/flush循环，不等待或重试满pipe，不更新liveness。具体sink的非阻塞/有界策略需单独
   审查；不可仅凭设置regular-file O_NONBLOCK就声称文件I/O严格有界。安全sink不可用时宁可无诊断，
   不改变原退出路径。首错记录本身不做I/O，既有watchdog仍是原边界。

备选仅输出UiFailure枚举更小，但无法区分多个GetProperty/校验失败，不足以确认此根因。
不建议额外GetAtomName/枚举所有property或完整X协议trace：扩大I/O、数据暴露和观察扰动。

## 什么新trace才足以确认

若未来单独获准的同profile诊断执行捕获以下同一调用链，才可将该次失败归因到缺失atom：

- connect完成；desktop InternAtom实际only_if_exists=true，返回NONE；
- facts.desktop GetProperty实际property=NONE，返回protocol Atom error、major20；
- 相同错误传播到startup终止，未被EOF/transport竞争取代；无Ready，保持原错误/rc。

以上只是期待的证据形状，**不是本次已观测字段**。若trace指向别的操作，则按真实首错继续调查，
不可用纯atom RED强行解释GUI失败。即使以后atom修复/纯测通过，也仍需独立实际link与UI资格。
