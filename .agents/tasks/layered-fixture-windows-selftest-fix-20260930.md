协调者补充确定Windows真机自检结果，供当前fixture修复线程处理（这个任务留给同一个writer，不另开重叠写入者）。在 out-fixed 构建CS1593已修后，真实 --self-test exit1，3/40 failures，全文 .agents/runs/layered-win-selftest-full-20260930.log：
1. non-crlf-payload-equals-expected：Cases.cs:264 的谓词 all.All(c => !c.Payload.Contains("\r\n") || c.Payload == c.ExpectedText) 把CRLF case要求相等了。逻辑应是 containsCRLF || payload==expected（只对非CRLF约束），补可证明的反例断言，不改payload目录。
2. known-legacy-38-scalars：当前 Count(ch => ch >=0xD800 && ch<=0xDFFF) 统计两种surrogate，39UTF16减2得37而非38。用高代理数/合法surrogatepair计数，补BMP+nonBMP回归。
3. punctuation-no-emoji：读当前谓词，禁止误把中文标点U+FF00范围都视作emoji。结合emoji检测标量边界与已有purecases，修测试正确约束，不删除该覆盖或变恒true。
请在同一fixture写入范围修这些真正测试逻辑错误；mac已通过不能推测C#自检。完成后coord会重新scp+C#编译+self-test+exportparity，artifact新版本不覆盖旧错误证据。
