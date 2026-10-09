# Windows first-trial accounting — Stage A sol 交付

状态：**SOURCE_FROZEN / STOP，等待 StageB 明确许可**。
冻结时间（UTC）：2026-09-30T19:34:27.619090+00:00。

## 范围

重读更新任务与计划，采用固定 pointer4 / multiclick1 / drag1 / scroll2 /
keyboard3 / known-input1：12 run组、120 planned slot、60 semantic case。
eligible95：move12 / click25 / drag10 / scroll14 / text_input10 / key_chord12 /
key_hold12；另外25非计数slot保留分母。variant逐项保留，仅承诺基础action
种类的最低10次，不宣称每按钮×count×方向都有10次。

只新增两个显式 NotImplementedError API stub、精确 API 合同、独立 canonical
JSON 测试输入及36个 unittest 需求方法。固定 JSON 是测试源码数据，不是运行时
计划实现，不是 GUI 证据。summary 测试直接从固定 JSON 取 manifest，不调用
build_manifest 前置阻断 summarize。synthetic runtime 身份在测试中显式标记。
测试包括空记录、第一失败/重试/缺失、非输入、重复/冲突/重放、异case绑定、
严格类型与边界、canonical 防篡改、两模式分离、variants 和永久false验收门禁。

keyboard01/02/07/08 是hold，02两个调用只计一slot；known09保留CRLF且引用
windows-winforms-known09-crlf-v1，保留NBSP/NFD/非BMP/Tab。不存在key_press。

## 唯一执行的源码验证

```sh
PYTHONPYCACHEPREFIX=/tmp/windows-first-trial-stage-a-pycache.jFjzFS python3 -m py_compile acceptance-fixture/trials/__init__.py tests/windows_first_trial_accounting.py
```

返回exit0，编译缓存在新建 /tmp 目录。**未运行 unittest、未调用 API、未运行
Windows/GUI/SSH/socket、未启动 CC/subagent、未review他人任务；未使用worktree、
commit/push/reset。** 现有产品、fixtures、分析器、旧tests、scripts/runner及大量
未提交源码均未编辑。源码文件最大474行。compile-only不证明任何测试通过。

## 预期真实RED（仅CC＋GLM执行）

36个需求方法均应触达至少一个缺失API；MISSING_CAPABILITY[build_manifest] 或
MISSING_CAPABILITY[summarize] 的 NotImplementedError 被 wrapper 转为 unittest
FAIL。ValueError验证用例同样不能把stub误判为成功。Import/permission/file/环境
异常保持ERROR，不转换为能力缺失或skip/pass。此处仅记录预期，未取得真实RED。
源码含 __main__ 入口供CC在真实Windows runner执行；本实现者没有执行它。

## 新源码与SHA256

| 路径 | SHA256 |
|---|---|
| `acceptance-fixture/trials/API.md` | `dce62b6d631ae50bf78b14ef40ac055b59fab081d32c3edf7d5e39c4715ea408` |
| `acceptance-fixture/trials/__init__.py` | `8502c470ecde47253a8c5a87ca4659c76ff2eb7b734831f3b8b77ac0721332b4` |
| `acceptance-fixture/trials/canonical-fixture.json` | `6834b40f30486c2bfc70e181bbbefc4f401ec9fdc03f321ddbc0d89490fa1344` |
| `tests/windows_first_trial_accounting.py` | `86a6b5f0d4ce0fdc214a338fd33332a81e700fda297e7931e4a25e2a4714dce0` |

另新增本报告与 `.agents/runs/windows-first-trial-stage-a-sol-20260930/`：
- `compile-command.txt`、`compile-result.txt`
- `source-snapshot/`：逐字节快照（包含新源码和15个只读输入）
- `SOURCE_SHA256SUMS.txt`：全部19个源码/输入的SHA清单
- `stage-a-source.tar.gz`：同一19文件打包
- `FREEZE.json`：冻结元数据、逐文件hash及角色、tar/hash清单摘要

只读输入包含9个 catalog/policy/task 引用，Windows runner两个文件，更新任务、
本计划及complete-actions计划/spec。源码快照哈希与原始字节核对；未改动原输入
权限或内容。未来源漂移必须停止原campaign，不能悄悄续用。

Archive SHA256：`062d755b207e649517767eb68b86a2cb1bed1218e03a60d16d962a2090352324`

SOURCE_SHA256SUMS SHA256：`61cbee297fa4c3846781d906ffd06d8946c75a88e6a45429054305b512fe3991`

Canonical plan digest：`b75f02c9ca38d375e6193384ce4e9a4983de7d39f7b904448e87c9ca3509bbe2`

报告是冻结交付说明，不属于被测源码tar（避免自引用哈希）。
尚未实现计划生成、统计器、CLI、README或新known-input任务；均留StageB。
真实120slot未执行、GUI未核验、action_trial_gate未满足。

**SOURCE_FROZEN — STOP；等待CC＋GLM真实Windows RED和StageB许可。**
