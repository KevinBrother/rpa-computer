只读复核修复后分析器，写入仅 .agents/reviews/layered-evidence-final-20260930.md。不GUI/不改source/不commit。请检查上轮F1–F4及当前26测试真实通过，确认盲N配对已移除：case1漏check不偏移case2，未知/同秒多候选不会exact，known原始CRLF→LF合法，NFC/NFD/NBSP严格，first-pass不被重试提高。
另外要核对真实fixture export的 schema（.agents/runs/layered-fixture-build-20260930/cases-manifest-macos.json），当前运行coord会生成 canonical catalog为 {cases:[{suite,case_id,task_payload,expected_text}]}，所以不要把原manifest不支持误判为运行阻塞，但要指出CLI支持哪些格式。最终result/工具策略是否由分析器覆盖：如果不覆盖明确在报告写出，coord另跑strict audit，不能自动将case通过当完整run通过。
执行pure tests，必要时stdin反例，给简短具体门禁结论/行号。不要重复泛化审查，聚焦修复是否实际生效。
