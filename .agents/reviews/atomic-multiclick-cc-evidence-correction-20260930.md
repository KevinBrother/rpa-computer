# 原子多击CC首轮报告：证据范围纠正

保留原报告/失败日志不改写。协调者核对真实测试代码后发现原报告第二节第6项有不成立的推断：它称“元数据保留、后续对跳过、cleanup释放等行为断言全部先于194行且通过”。

实际 `src/runtime/session/tests/multiclick.rs`：`record.error.code` 断言在194行，`calls`完整序列、`cleanup_calls`、`held_snapshot`等断言位于其后；首个错误码断言失败时，这些断言未执行，外层3×2循环也未跑全。因此第一轮只能证实当前迭代的input_outcome/cleanup_outcome前置断言，不能把该测试预期覆盖范围当已执行结果。

已确认源码中的BackendError `input_failed` -> Runtime `input_error`映射解释当前断言失败，修复仅测试边界预期是合理的；但是否还存在后续trace/metadata缺陷，必须等CC完整复测后判断。源码审查与运行覆盖分开记录。本说明不否定native crate72个已通过纯测试，也不代表GUI通过。
