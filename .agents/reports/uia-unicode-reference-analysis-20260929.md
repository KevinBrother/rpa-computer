# uia 仓库 Unicode 输入参考分析（2026-09-29）

> 后续校正（2026-09-29）：用户明确 pyrpa 已废弃，不再作为当前 Studio 输入链路依据。用户在当前 Studio Block 观察到 delay=0 重复、delay=0.001 正常；此为用户报告，尚未由协调者在相同运行版本上复测。已读当前 Studio 非deprecated Block（`studio/src/render/pages/visual/workbench/menu/block/library/windows/children/keyboard/blocks/write.tsx:27-34`）：生成 `rpa.ui.keyboard_action.keyboard_write`。SDK Windows分支（`sdk/rpa/ui/keyboard_action.py:84-86`）调用第三方Python `keyboard.write`，不是本报告以下分析的uia C++ SendText路径。SDK `uv.lock`锁定keyboard0.13.5；用户实际Executor安装版本尚未核实。
> 检查上游v0.13.5源码：Windows默认exact=True，delay=0与非0都按Python字符循环；唯一delay分支为 `if delay: time.sleep(delay)`，并非0整串、非0逐字。输入前stash_state、结束restore_modifiers；换行和退格走send(letter)，其他字符走type_unicode。`_winkeyboard.type_unicode` 对一个码点按 presses + releases 组装事件：非BMP为高down、低down、高up、低up；我们C#probe则是高down、高up、低down、低up。此序列差异是新待验证因素，不是已证明的根因。1ms成功与C#10ms部分漏字不能直接按数字比较：路径、事件顺序、状态处理不同。后续应单变量配对比较事件顺序与实际发送间隔，保留无emoji对照。
> 上游核对来源：boppreh/keyboard tag v0.13.5，keyboard/__init__.py:819-872、keyboard/_winkeyboard.py:596-613。以下内容保留作为历史静态分析，不代表当前Studio输入实现或已验证修复。

范围：只读检查 `/Volumes/doc/workspace/datagrand/rpa/uia` 本地 `build-container` 分支，HEAD `3bf21a5db270a909c27cebc288a49d783d980746`。未拉取远端，未运行桌面测试，未修改 uia；不代表其他分支或已部署二进制。

## 结论

存在相关机制，但不能称为已证明解决本轮现代 Notepad 的重复/漏字问题。最值得借鉴的是：按 Unicode 码点逐次发送、可配置间隔、正确成对的 UTF-16 down/up，以及把原生输入和 UIA 赋值区分为不同能力。

## 三条不同路径

1. `src/features/pyrpa/keyboard.cc:61-92`：`Keyboard::Write` 对有效UTF-8按码点拆分，每个码点单独 `keyboard_->Send(s)`，随后sleep `int(delay*1000)` 毫秒。delay单位是秒。`src/features/pyrpa/block/keyboard.py:153-159` 包装默认delay=0；不能说默认500ms。C++绑定见 `src/features/pyrpa/main.cc:59-66`。
2. `src/providers/windows_host/keyboard.cc:202-225`：`Keyboard::Send(text)` 转UTF-16，对每个code unit生成同wScan的Unicode down/up，积累整串后一次SendInput。无逐字符等待、无目标文本回读。v2 gRPC `SendText`（`src/server/services/v2/input.cc:169-171`）直接进入此路径；`RPA::SendText`（`src/features/pyrpa/pyrpa.cc:56-58`）也没有经过Write。
3. `src/server/services/v1/giant_service.cc:300-315`：`SetNodeText` 查ValuePattern后SetValue；Windows实现 `src/providers/uia_provider/pattern/value_provider.cc:21-24` 调用IUIAutomationValuePattern，不经SendInput。这是能力可用时的替代通道，不是通用原生键盘路径的修复。当前SetValue包装未检查HRESULT或回读，不能照抄成功判定。

## 借鉴点与限制

- Unicode scalar为发送组：中文的UTF-8多字节不拆坏；非BMP scalar转成两个UTF-16单元，四个down/up事件同一SendInput组发送。按码点不等于按grapheme cluster；组合emoji可能拆成多个组。
- 同一UTF-16单元keydown/keyup保持wScan一致，比当前enigo0.3.0代理对第二单元keyup错用首单元更规范。但我们的独立C#探针已经这样构造，仍有失败，故非充分修复。
- 此处也没有enigo的Return后额外Unicode LF双路径；但没有独立Enter/Tab规范化策略，不能据此宣称多行输入通用正确。
- 可配置节奏值得对照验证；本轮char0/char10仍出现失败，不能由源码推断uia逐字符必定可靠。其500ms示例是值得实验的新参数，不是已验证方案。
- 不应照搬手写UTF-8 splitter：它按首字节猜长度，未完整验证continuation bytes及剩余字节边界。Rust可用str.chars()/char.encode_utf16()实现有类型保障的拆分。
- 不应照搬阻塞sleep和无界整串发送：computer Runtime需要取消检查、deadline与明确partial结果。
- 不应将UIA SetValue悄悄伪装成text_input；保留键鼠兜底通道，结构化赋值作为独立语义能力，不能用来代替本轮原生输入验收。

## 历史与测试证据

- `e2139024`，2022-06-22 `Add SendText support.`：已经是当前整串Unicode事件数组+一次SendInput模式。
- `2a069d8e`，2022-08-23 `keyboard api`：加入按码点拆分Write与延迟；提交内容未证明专门修复本次同类Notepad错误。
- `tests/pyrpa/test_keyboard.py:23-24`：`keyboard.write("你🏳️好😂啊🏳️‍⚧️𒁦", 0.5)`，覆盖复杂字符样例但没有读取目标文本、内容断言或重复成功率统计；不足以证明文本正确。
- `71a393f7`，2025-03-21 `fix(chrome):消息输入中文问题，chrome树兜底`：相关编码改动是Chrome消息从atob改Base64.decode（当前 `src/providers/chrome/websocket_server.cc:339`），不是SendInput修复，不能混为一谈。

## 建议的下一步（尚未执行）

在自有测试窗口对原批量、逐scalar零延迟、10/50/100/500ms进行有界对照；相同文本、配对编辑控件、每次实际文本回读、保留全部失败，控制顺序/状态。优先验证uia所体现的发送节奏，而不是迁入整套RPA/UIA依赖。若有效，再在Windows原生backend实现可取消的发送策略并回归其他平台契约。

结论边界：之前只能确认原生SendInput路径也能复现，尚未证明是Windows SendInput自身缺陷；本次静态分析没有改变这个边界。
